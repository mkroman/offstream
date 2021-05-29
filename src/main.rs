#![allow(clippy::pub_enum_variant_names)]

use std::time::Duration;

use clap::Clap;
use color_eyre::eyre::Error as EyreError;
use log::{debug, error};
use serde_json::{Map, Value as JsonValue};
use tokio::time::sleep;

mod cli;
mod client;
mod database;
mod error;

use client::Client;
use database::Database;
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

async fn download_film(
    client: &Client,
    db: &Database,
    film_data: &Map<String, JsonValue>,
) -> Option<()> {
    debug!(
        "Downloading film {}",
        film_data
            .get("title")
            .and_then(JsonValue::as_str)
            .unwrap_or("?")
    );

    let film_id = film_data.get("id").and_then(JsonValue::as_u64)?;
    let film = client.get_film(film_id).await.ok()?;
    let film_data = &film.data;

    // Insert the film into the database
    if db
        .create_film(
            film_id,
            film_data.title(),
            film.data.original_title(),
            film_data.director(),
            film_data.production_year(),
            film_data.duration(),
            film_data.description(),
            film_data.age_restriction(),
        )
        .is_err()
    {
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

    Some(())
}

async fn download_films(client: &Client, db: &Database, films: JsonValue) -> Option<()> {
    let data = films.get("data")?.as_object()?;

    debug!("Received a list containing {} films", data.len());

    let film_ids: Vec<&str> = data.keys().map(String::as_str).collect();
    let missing_film_ids = film_ids_not_in_db(db, &film_ids).ok()?;

    debug!(
        "Of those films, {} are not present in our database",
        missing_film_ids.len()
    );

    for missing_id in missing_film_ids {
        if let Some(object) = data.get(missing_id).and_then(JsonValue::as_object) {
            download_film(client, db, object).await;

            sleep(Duration::from_secs(1)).await;
        } else {
            error!("Could not extract data field from film object");
        }
    }

    Some(())
}

#[tokio::main]
async fn main() -> Result<(), EyreError> {
    env_logger::init();
    color_eyre::install()?;

    let _opts = cli::Opts::parse();
    let mut client = Client::new().unwrap();
    let db = database::open("test.db")?;

    client.update_xsrf_token().await?;
    // Fetch a list of films
    let films = client.get_films().await?;

    // Download all missing films
    download_films(&client, &db, films).await;

    Ok(())
}
