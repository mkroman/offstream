use std::{ops::Deref, path::Path};

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use tracing::{instrument, trace};

use crate::client::{FilmCountry, FilmGenre, FilmYear, GetFilmResponseData, GetFilmResponseStatus};
use crate::Error;

#[derive(Debug)]
pub struct Database(Connection);

#[derive(Debug)]
pub struct FilmStatus {
    pub film_id: u64,
    pub status: Option<String>,
    pub vimeo_id: String,
    pub greeting_vimeo_id: Option<String>,
}

#[derive(Debug)]
pub struct MissingFilmDownload {
    pub id: u64,
    pub title: String,
    pub original_title: Option<String>,
    pub director: String,
    pub production_year: u64,
}

#[derive(Debug)]
pub struct FilmDownload {
    pub id: u64,
    pub film_id: u64,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub path: String,
}

/// Opens and initializes a database
pub fn open<P: AsRef<Path>>(path: P) -> Result<Database, rusqlite::Error> {
    let db = Database::open(path)?;

    trace!("Setting up initial SQL");
    db.execute_batch(include_str!("init.sql"))?;

    Ok(db)
}

impl Database {
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Database, rusqlite::Error> {
        let conn = Connection::open(path)?;

        Ok(Database(conn))
    }

    /// Inserts a new film into the database.
    #[instrument(err, skip(self))]
    pub fn create_film(&self, id: u64, film_data: &GetFilmResponseData) -> Result<(), Error> {
        trace!("Creating film entry");

        self.execute(
            "
            INSERT INTO films
                (id, title, original_title, director, production_year, duration, description, age_restriction)
            VALUES
                (?, ?, ?, ?, ?, ?, ?, ?)
            ",
            params!(
                id,
                film_data.title,
                film_data.original_title,
                film_data.director,
                film_data.production_year,
                film_data.duration,
                film_data.description,
                film_data.age_restriction
            ),
        )?;

        Ok(())
    }

    /// Inserts a new thumbnail into the database.
    #[instrument(err, skip(self))]
    pub fn create_film_thumbnail(
        &self,
        film_id: u64,
        resolution: &str,
        url: Option<&str>,
    ) -> Result<(), Error> {
        trace!("Creating thumbnail entry");

        self.execute(
            "
            INSERT INTO film_thumbnails
                (film_id, resolution, url)
            VALUES
                (?, ?, ?)
            ",
            params!(film_id, resolution, url),
        )?;

        Ok(())
    }

    /// Returns a list of thumbnails in the tuple format `(resolution, url)` if any.
    #[instrument(err, skip(self))]
    pub fn get_film_thumbnails(&self, film_id: u64) -> Result<Vec<(String, String)>, Error> {
        trace!("Querying for film thumbnails");

        let mut stmt =
            self.prepare("SELECT resolution, url FROM film_thumbnails WHERE film_id = ?")?;

        let thumbs = stmt
            .query_map([film_id], |row| -> Result<(_, _), _> {
                Ok((row.get(0)?, row.get(1)?))
            })?
            .filter_map(Result::ok)
            .collect::<Vec<_>>();

        trace!(res = ?thumbs);

        Ok(thumbs)
    }

    /// Returns a film status for a given `film_id`.
    #[instrument(err, skip(self))]
    pub fn get_film_status(&self, film_id: u64) -> Result<FilmStatus, Error> {
        trace!("Querying for film status");

        let mut stmt = self.prepare(
            "SELECT film_id, status, vimeo_id, greeting_vimeo_id
                FROM film_status
                WHERE film_id = ? AND vimeo_id IS NOT NULL",
        )?;

        let res = stmt.query_row([film_id], |row| {
            Ok(FilmStatus {
                film_id: row.get(0)?,
                status: row.get(1)?,
                vimeo_id: row.get(2)?,
                greeting_vimeo_id: row.get(3)?,
            })
        })?;

        trace!(result = ?res);

        Ok(res)
    }

    /// Creates a new genre.
    #[instrument(err, skip(self))]
    pub fn create_genre(&self, id: &str, title: &str) -> Result<(), Error> {
        trace!("Creating genre entry");

        self.execute(
            "
            INSERT INTO genres
                (identifier, title)
            VALUES
                (?, ?)
            ",
            [id, title],
        )?;

        Ok(())
    }

    /// Creates a new country.
    #[instrument(err, skip(self))]
    pub fn create_country(&self, title: &str, code: &str) -> Result<(), Error> {
        trace!("Creating country entry");

        self.execute(
            "
            INSERT INTO countries
                (title, code)
            VALUES
                (?, ?)
            ",
            [title, code],
        )?;

        trace!(id = self.last_insert_rowid());

        Ok(())
    }

    /// Returns a genres id, given its identifier.
    #[instrument(err, skip(self))]
    pub fn get_genre(&self, identifier: &str) -> Result<Option<u64>, Error> {
        trace!("Querying for genre");

        let id = self.query_row(
            "SELECT id FROM genres WHERE identifier = ?",
            [identifier],
            |row| row.get(0),
        )?;

        trace!(result = ?id);

        Ok(id)
    }

    /// Creates an association between a film and a genre.
    #[instrument(err, skip(self))]
    pub fn create_film_genre(&self, film_id: u64, genre_id: u64) -> Result<(), Error> {
        trace!("Creating genre and film association");

        self.execute(
            "
            INSERT INTO film_genres
                (film_id, genre_id)
            VALUES
                (?, ?)
            ",
            [film_id, genre_id],
        )?;

        Ok(())
    }

    /// Returns a countrys id, given its code.
    #[instrument(err, skip(self))]
    pub fn get_country(&self, code: &str) -> Result<Option<u64>, Error> {
        trace!("Querying for country");

        let id = self.query_row("SELECT id FROM countries WHERE code = ?", [code], |row| {
            row.get(0)
        })?;

        trace!(result = ?id);

        Ok(id)
    }

    /// Creates an association between a film and a country.
    #[instrument(err, skip(self))]
    pub fn create_film_country(&self, film_id: u64, country_id: u64) -> Result<(), Error> {
        trace!("Creating country and film association");

        self.execute(
            "
            INSERT INTO film_countries
                (film_id, country_id)
            VALUES
                (?, ?)
            ",
            [film_id, country_id],
        )?;

        Ok(())
    }

    /// Creates an association between a film and a country.
    #[instrument(err, skip(self))]
    pub fn create_film_year(&self, film_id: u64, year: &FilmYear) -> Result<(), Error> {
        trace!("Creating film year",);

        self.execute(
            "
            INSERT INTO film_years
                (id, film_id, title, product_id)
            VALUES
                (?, ?, ?, ?)
            ",
            params!(year.id, film_id, year.title, year.product_id),
        )?;

        Ok(())
    }

    /// Creates a film status for a given `film_id`.
    #[instrument(err, skip(self))]
    pub fn create_film_status(
        &self,
        film_id: u64,
        status: &GetFilmResponseStatus,
    ) -> Result<(), Error> {
        trace!("Creating film status");

        self.execute(
            "
            INSERT INTO film_status
                (film_id, status, vimeo_id, greeting_vimeo_id)
            VALUES
                (?, ?, ?, ?)
            ",
            params!(
                film_id,
                status.status,
                status.vimeo_id,
                status.greeting_vimeo_id
            ),
        )?;

        Ok(())
    }

    /// Creates a film competition for a given `film_id`.
    #[instrument(err, skip(self))]
    pub fn create_film_competition(&self, film_id: u64, competition: &str) -> Result<(), Error> {
        trace!("Creating film competition");

        self.execute(
            "
            INSERT INTO film_competitions
                (film_id, name)
            VALUES
                (?, ?)
            ",
            params!(film_id, competition),
        )?;

        Ok(())
    }

    /// Inserts any missing `genres` into the database if not present.
    pub fn sync_genres(&self, genres: &[FilmGenre]) -> Result<(), Error> {
        let mut stmt = self.prepare("SELECT identifier FROM genres WHERE identifier = ?")?;
        let existing_genres: Vec<String> = genres
            .iter()
            .map(|genre| -> Result<String, _> { stmt.query_row([genre.id()], |row| row.get(0)) })
            .filter_map(Result::ok)
            .collect();

        for genre in genres.iter() {
            if !existing_genres.iter().any(|x| x == &genre.id) {
                trace!(
                    genre.id = genre.id.as_str(),
                    genre.title = genre.title.as_str(),
                    "Creating new genre"
                );

                self.create_genre(genre.id(), genre.title())?;
            }
        }

        Ok(())
    }

    /// Inserts any missing `countries` into the database if not present.
    pub fn sync_countries(&self, countries: &[FilmCountry]) -> Result<(), Error> {
        let mut stmt = self.prepare("SELECT title, code FROM countries WHERE code = ?")?;

        let existing_countries: Vec<String> = countries
            .iter()
            .map(|country| -> Result<String, _> {
                stmt.query_row([country.code()], |row| row.get(1))
            })
            .filter_map(Result::ok)
            .collect();

        for country in countries.iter() {
            if !existing_countries.iter().any(|x| x == &country.code) {
                trace!(
                    country.title = country.title.as_str(),
                    country.code = country.title.as_str(),
                    "Creating new country entry since it's not already present"
                );

                self.create_country(country.title(), country.code())?;
            }
        }

        Ok(())
    }

    pub fn get_missing_downloads(&self) -> Result<Vec<MissingFilmDownload>, Error> {
        let mut stmt = self.prepare(
            "SELECT f.id, f.title, f.original_title, f.director, f.production_year
            FROM films AS f
            LEFT JOIN film_downloads AS dl
            ON f.id = dl.film_id
            WHERE dl.id IS NULL OR dl.finished_at IS NULL",
        )?;

        let res = stmt
            .query_map([], |row| {
                Ok(MissingFilmDownload {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    original_title: row.get(2)?,
                    director: row.get(3)?,
                    production_year: row.get(4)?,
                })
            })?
            .filter_map(Result::ok)
            .collect();

        Ok(res)
    }

    /// Inserts or updates a film download with the given `completed` info.
    pub fn upsert_film_download(
        &self,
        film_id: u64,
        completed: bool,
        path: Option<&str>,
    ) -> Result<(), Error> {
        let finished_at = if completed { Some(Utc::now()) } else { None };

        self.execute(
            "
            REPLACE INTO film_downloads
            (film_id, finished_at, path)
            VALUES
            (?, ?, ?)
            ",
            params!(film_id, finished_at, path),
        )?;

        Ok(())
    }
}

impl Deref for Database {
    type Target = Connection;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
