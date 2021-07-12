#![allow(clippy::pub_enum_variant_names)]

use std::env;
use std::path::Path;
use std::time::Duration;

use clap::Clap;
use color_eyre::eyre::{self, Error as EyreError};
use serde_json::{Map, Value as JsonValue};
use tokio::process::Command;
use tokio::time::sleep;
use tracing::{debug, debug_span, error, instrument, trace};
use tracing_error::ErrorLayer;
use tracing_subscriber::layer::SubscriberExt;

mod cli;
mod client;
mod database;
mod error;

use client::Client;
use database::{Database, MissingFilmDownload};
use error::{Error, ErrorKind};

/// Given a list of `film_ids`, this will return a new list that only consists of id's not
/// currently present in the database.
fn film_ids_not_in_db<'a>(db: &Database, film_ids: &[&'a str]) -> Result<Vec<&'a str>, Error> {
    let mut res = vec![];
    let mut stmt = db.prepare("SELECT id FROM films WHERE id = ?1")?;

    for id in film_ids {
        if stmt.query([id])?.next()?.is_none() {
            res.push(*id);
        }
    }

    Ok(res)
}

#[instrument(skip(client, db), err)]
async fn fetch_film(
    client: &Client,
    db: &Database,
    film_data: &Map<String, JsonValue>,
) -> Result<(), Error> {
    let film_id = film_data.get("id").and_then(JsonValue::as_u64).unwrap();

    debug!("Getting film details");
    let film = client.get_film(film_id).await?;
    let film_data = &film.data;

    // Insert the film into the database
    db.create_film(film_id, film_data)?;

    // Insert thumbnails into the database
    if !film_data.thumbnails.is_empty() {
        if let Ok(stored_thumbnails) = db.get_film_thumbnails(film_id) {
            for (key, value) in &film_data.thumbnails {
                if !stored_thumbnails
                    .iter()
                    .any(|(ref resolution, _url)| resolution == key)
                {
                    // Insert the thumbnail
                    match db.create_film_thumbnail(film_id, key, Some(value.as_str())) {
                        Ok(_) => {}
                        Err(err) => error!(?err, "Could not add thumbnail"),
                    }
                }
            }
        }
    }

    if !film_data.genres.is_empty() {
        if let Err(err) = db.sync_genres(&film_data.genres) {
            error!("Could not upsert genres: {:?}", err);
        }

        // Create genre associations
        for genre in &film_data.genres {
            if let Ok(Some(genre_id)) = db.get_genre(genre.id()) {
                if let Err(err) = db.create_film_genre(film_id, genre_id) {
                    error!("Could not associate film with genre: {:?}", err);
                }
            } else {
                error!("Could not find genre in database: {}", genre.id);
            }
        }
    }

    if !film_data.countries.is_empty() {
        if let Err(err) = db.sync_countries(&film_data.countries) {
            error!("Could not upsert countries: {:?}", err);
        }

        // Create country associations
        for country in &film_data.countries {
            if let Ok(Some(country_id)) = db.get_country(country.code()) {
                if let Err(err) = db.create_film_country(film_id, country_id) {
                    error!("Could not associate film with country: {:?}", err);
                }
            } else {
                error!("Could not find country in database: {}", country.code);
            }
        }
    }

    if !film_data.competitions.is_empty() {
        for competition in &film_data.competitions {
            if let Err(err) = db.create_film_competition(film_id, competition.as_str()) {
                error!("Could not create film competition: {:?}", err);
            }
        }
    }

    if let Err(err) = db.create_film_year(film_id, &film_data.year) {
        error!("Could not create film year: {:?}", err);
    }

    if let Err(err) = db.create_film_status(film_id, &film.status) {
        error!("Could not create film status: {:?}", err);
    }

    Ok(())
}

/// Downloads all films that we have fetched data for, that aren't already downloaded.
#[instrument(skip(db))]
async fn download_missing_films(db: &Database) -> Result<(), Error> {
    let missing_downloads = db.get_missing_downloads()?;
    let num_missing_downloads = missing_downloads.len();

    if num_missing_downloads > 0 {
        debug!(num_missing_downloads, "Starting download of missing films");

        for missing_download in missing_downloads {
            if let Err(err) = download_film(db, &missing_download).await {
                error!(
                    film_id = missing_download.id,
                    film_title = missing_download.title.as_str(),
                    "Could not download film"
                );
                eprintln!("{:?}", eyre::Report::new(err));
            }
        }
    }

    Ok(())
}

#[instrument(skip(client, db, films), err)]
async fn fetch_films(client: &Client, db: &Database, films: JsonValue) -> Result<(), Error> {
    let data = films
        .get("data")
        .and_then(JsonValue::as_object)
        .ok_or_else(|| {
            Error::from(ErrorKind::ApiError(
                "API response did not include a .data field".to_string(),
            ))
        })?;

    let num_films = data.len();
    debug!("Received a list containing {} films", num_films);

    let film_ids: Vec<&str> = data.keys().map(String::as_str).collect();
    let missing_film_ids = film_ids_not_in_db(db, &film_ids)?;

    debug!(
        "Of those films, {} are not present in our database",
        missing_film_ids.len()
    );

    for missing_id in missing_film_ids {
        if let Some(object) = data.get(missing_id).and_then(JsonValue::as_object) {
            if let Err(error) = fetch_film(client, db, object).await {
                error!(
                    film_id = missing_id,
                    error = ?error,
                    "Could not fetch the film"
                );
                eprintln!("{:?}", eyre::Report::new(error));
            }

            sleep(Duration::from_secs(1)).await;
        } else {
            error!("Could not extract data field from film object");
        }
    }

    Ok(())
}

#[instrument(skip(db), err)]
async fn download_film(db: &Database, film: &MissingFilmDownload) -> Result<(), Error> {
    let film_status = db.get_film_status(film.id)?;

    let vimeo_url = format!(
        "https://player.vimeo.com/video/{}?app_id=122963",
        film_status.vimeo_id
    );
    let filename = format!(
        "{} - {} ({}).mp4",
        film.director, film.title, film.production_year
    );
    let output_path = Path::new("films")
        .join(film.production_year.to_string())
        .join(filename);
    let output_path_str = output_path.to_string_lossy().into_owned();

    let span = debug_span!(
        "download_film",
        film_id = film.id,
        film_title = film.title.as_str()
    );
    let _enter = span.enter();

    debug!(
        film_director = film.director.as_str(),
        ?output_path,
        "Downloading film"
    );

    let args = [
        "--referer",
        "https://offstream.dk/",
        "-f",
        "bestvideo+bestaudio",
        "--merge-output-format",
        "mp4",
        "-o",
        output_path_str.as_str(),
        vimeo_url.as_str(),
    ];

    debug!(?args, "Running youtube-dl");

    let mut cmd = Command::new("youtube-dl");
    cmd.args(args.iter());

    db.upsert_film_download(film.id, false, Some(output_path_str.as_str()))?;

    let res = cmd
        .spawn()
        .map_err(|err| {
            Error::from(ErrorKind::YouTubeDlError(format!(
                "Could not create process: {}",
                err
            )))
        })?
        .wait()
        .await;

    if let Ok(exit) = res {
        if exit.success() {
            debug!("youtube-dl finished successfully");

            db.upsert_film_download(film.id, true, Some(output_path_str.as_str()))?;
        } else {
            debug!("youtube-dl failed: {}", exit);
        }
    }

    Ok(())
}

fn init_tracing(jaeger_opts: cli::JaegerOpts) -> Result<(), EyreError> {
    if jaeger_opts.enabled {
        // Install a new OpenTelemetry trace pipeline
        let tracer = opentelemetry_jaeger::new_pipeline()
            .with_service_name(jaeger_opts.service_name)
            .install_batch(opentelemetry::runtime::Tokio)?;

        // Create a tracing layer with the configured tracer
        let telemetry = tracing_opentelemetry::layer().with_tracer(tracer);
        let collector = tracing_subscriber::Registry::default()
            .with(ErrorLayer::default())
            .with(telemetry);

        tracing::subscriber::set_global_default(collector)
            .expect("Unable to set a global collector");
    } else {
        tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .pretty()
            .init();
    }

    let build = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };
    trace!(build, version = env!("CARGO_PKG_VERSION"), "init");

    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), EyreError> {
    color_eyre::install()?;

    let opts = cli::Opts::parse();

    // Set up stdout or jaeger tracing
    init_tracing(opts.jaeger_opts)?;

    let db = database::open(opts.database_path)?;
    let mut client = Client::new().unwrap();

    client.update_xsrf_token().await?;

    // Fetch a list of films
    let films = client.get_films().await?;
    trace!(?films, "Retrieved list of films");

    // Fetch all missing films
    fetch_films(&client, &db, films).await?;

    // Download all films not already downloaded
    download_missing_films(&db).await?;

    Ok(())
}
