#![allow(clippy::pub_enum_variant_names)]

use std::path::Path;
use std::time::Duration;

use clap::Clap;
use color_eyre::eyre::{self, Error as EyreError};
use log::{debug, error, info};
use serde_json::{Map, Value as JsonValue};
use tokio::process::Command;
use tokio::time::sleep;

mod cli;
mod client;
mod database;
mod error;

use client::Client;
use database::{Database, MissingFilmDownload};
use error::Error;

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

async fn fetch_film(
    client: &Client,
    db: &Database,
    film_data: &Map<String, JsonValue>,
) -> Result<(), Error> {
    debug!(
        "Downloading film {}",
        film_data
            .get("title")
            .and_then(JsonValue::as_str)
            .unwrap_or("?")
    );

    let film_id = film_data
        .get("id")
        .and_then(JsonValue::as_u64)
        .expect("json data does not have a valid id field");
    let film = client.get_film(film_id).await?;
    let film_data = &film.data;

    // Insert the film into the database
    if db.create_film(film_id, film_data).is_err() {
        error!("Could not insert film {}", film_id)
    }

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
                        Ok(_) => {
                            debug!("Added thumbnail for film {}: {} @ {}", film_id, key, value)
                        }
                        Err(err) => error!("Could not add thumbnail: {:?}", err),
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
async fn download_missing_films(db: &Database) -> Result<(), Error> {
    let missing_downloads = db.get_missing_downloads()?;

    println!("Missing downloads: {:#?}", missing_downloads);

    for missing_download in missing_downloads {
        if let Err(err) = download_film(db, &missing_download).await {
            error!(
                "Could not download film id={} title={}",
                missing_download.id, missing_download.title
            );
            eprintln!("{:?}", eyre::Report::new(err));
        }
    }

    Ok(())
}

async fn fetch_films(client: &Client, db: &Database, films: JsonValue) -> Result<(), Error> {
    let data = films
        .get("data")
        .and_then(JsonValue::as_object)
        .ok_or_else(|| Error::ApiError("API response did not include a .data field".to_string()))?;

    debug!("Received a list containing {} films", data.len());

    let film_ids: Vec<&str> = data.keys().map(String::as_str).collect();
    let missing_film_ids = film_ids_not_in_db(db, &film_ids)?;

    debug!(
        "Of those films, {} are not present in our database",
        missing_film_ids.len()
    );

    for missing_id in missing_film_ids {
        if let Some(object) = data.get(missing_id).and_then(JsonValue::as_object) {
            if let Err(err) = fetch_film(client, db, object).await {
                error!("Could not fetch the film with id={}", missing_id);
                eprintln!("{:?}", eyre::Report::new(err));
            }

            sleep(Duration::from_secs(1)).await;
        } else {
            error!("Could not extract data field from film object");
        }
    }

    Ok(())
}

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

    info!(
        "Downloading {} by {} to {}",
        film.title, film.director, output_path_str
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

    debug!("Running youtube-dl with the following arguments:");
    debug!("{:?}", args);

    let mut cmd = Command::new("youtube-dl");
    cmd.args(args.iter());

    db.upsert_film_download(film.id, false, Some(output_path_str.as_str()))?;

    let res = cmd
        .spawn()
        .map_err(|e| Error::YouTubeDlError(format!("Could not create process: {}", e)))?
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

#[tokio::main]
async fn main() -> Result<(), EyreError> {
    env_logger::init();
    color_eyre::install()?;

    let opts = cli::Opts::parse();
    let mut client = Client::new().unwrap();
    let db = database::open(opts.database_path)?;

    client.update_xsrf_token().await?;
    // Fetch a list of films
    let films = client.get_films().await?;

    // Fetch all missing films
    fetch_films(&client, &db, films).await?;

    // Download all films not already downloaded
    download_missing_films(&db).await?;

    Ok(())
}
