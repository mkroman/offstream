use std::{ops::Deref, path::Path};

use log::trace;
use rusqlite::{params, Connection};

use crate::client::{FilmCountry, FilmGenre, FilmYear, GetFilmResponseStatus};
use crate::Error;

pub struct Database(Connection);

/// Opens and initializes a database
pub fn open<P: AsRef<Path>>(path: P) -> Result<Database, rusqlite::Error> {
    let db = Database::open(path)?;

    db.execute_batch(
        "
        PRAGMA foreign_keys = ON;
        BEGIN;

        CREATE TABLE IF NOT EXISTS films (
            id INTEGER PRIMARY KEY,
            title VARCHAR,
            original_title VARCHAR,
            director VARCHAR,
            production_year INTEGER,
            duration INTEGER,
            description VARCHAR,
            age_restriction VARCHAR
        );

        CREATE TABLE IF NOT EXISTS film_status (
            film_id INTEGER REFERENCES films (id) ON DELETE CASCADE,
            status VARCHAR,
            vimeo_id VARCHAR,
            greeting_vimeo_id VARCHAR
        );

        CREATE TABLE IF NOT EXISTS film_thumbnails (
            id INTEGER PRIMARY KEY,
            film_id INTEGER REFERENCES films (id) ON DELETE CASCADE,
            resolution VARCHAR NOT NULL,
            url VARCHAR NOT NULL
        );

        CREATE TABLE IF NOT EXISTS countries (
            id INTEGER PRIMARY KEY,
            title VARCHAR,
            code VARCHAR
        );

        CREATE TABLE IF NOT EXISTS film_countries (
            film_id INTEGER REFERENCES films (id) ON DELETE CASCADE,
            country_id INTEGER REFERENCES countries (id) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS genres (
            id INTEGER PRIMARY KEY,
            identifier VARCHAR,
            title VARCHAR
        );

        CREATE TABLE IF NOT EXISTS film_genres (
            film_id INTEGER REFERENCES films (id) ON DELETE CASCADE,
            genre_id INTEGER REFERENCES genres (id) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS film_competitions (
            film_id INTEGER REFERENCES films (id) ON DELETE CASCADE,
            name VARCHAR
        );

        CREATE TABLE IF NOT EXISTS film_years (
            id INTEGER NOT NULL,
            film_id INTEGER REFERENCES films (id) ON DELETE CASCADE,
            title VARCHAR,
            product_id VARCHAR
        );

        CREATE INDEX IF NOT EXISTS idx_film_thumbnails ON film_thumbnails (film_id);
        CREATE INDEX IF NOT EXISTS idx_genres ON genres (identifier);
        CREATE INDEX IF NOT EXISTS idx_film_genres ON film_genres (film_id, genre_id);

        COMMIT;
        ",
    )?;

    Ok(db)
}

impl Database {
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Database, rusqlite::Error> {
        let conn = Connection::open(path)?;

        Ok(Database(conn))
    }

    /// Inserts a new film into the database.
    pub fn create_film(
        &self,
        id: u64,
        title: Option<&str>,
        original_title: Option<&str>,
        director: Option<&str>,
        production_year: Option<u64>,
        duration: Option<u64>,
        description: Option<&str>,
        age_restriction: Option<&str>,
    ) -> Result<(), Error> {
        self.execute(
            "
            INSERT INTO films
                (id, title, original_title, director, production_year, duration, description, age_restriction)
            VALUES
                (?, ?, ?, ?, ?, ?, ?, ?)
            ",
            params!(
                id,
                title,
                original_title,
                director,
                production_year,
                duration,
                description,
                age_restriction
            ),
        )?;

        Ok(())
    }

    /// Inserts a new thumbnail into the database.
    pub fn create_film_thumbnail(
        &self,
        film_id: u64,
        resolution: &str,
        url: Option<&str>,
    ) -> Result<(), Error> {
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
    pub fn get_film_thumbnails(&self, film_id: u64) -> Result<Vec<(String, String)>, Error> {
        let mut stmt =
            self.prepare("SELECT resolution, url FROM film_thumbnails WHERE film_id = ?")?;
        let thumbs = stmt
            .query_map([film_id], |row| -> Result<(_, _), _> {
                Ok((row.get(0)?, row.get(1)?))
            })?
            .filter_map(Result::ok)
            .collect::<Vec<_>>();

        Ok(thumbs)
    }

    /// Creates a new genre.
    pub fn create_genre(&self, id: &str, title: &str) -> Result<(), Error> {
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
    pub fn create_country(&self, title: &str, code: &str) -> Result<(), Error> {
        self.execute(
            "
            INSERT INTO countries
                (title, code)
            VALUES
                (?, ?)
            ",
            [title, code],
        )?;

        Ok(())
    }

    /// Returns a genres id, given its identifier.
    pub fn get_genre(&self, identifier: &str) -> Result<Option<u64>, Error> {
        let id = self.query_row(
            "SELECT id FROM genres WHERE identifier = ?",
            [identifier],
            |row| row.get(0),
        )?;

        Ok(id)
    }

    /// Creates an association between a film and a genre.
    pub fn create_film_genre(&self, film_id: u64, genre_id: u64) -> Result<(), Error> {
        trace!(
            "Creating genre association between film_id={} and genre_id={}",
            film_id,
            genre_id
        );

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
    pub fn get_country(&self, code: &str) -> Result<Option<u64>, Error> {
        let id = self.query_row("SELECT id FROM countries WHERE code = ?", [code], |row| {
            row.get(0)
        })?;

        Ok(id)
    }

    /// Creates an association between a film and a country.
    pub fn create_film_country(&self, film_id: u64, country_id: u64) -> Result<(), Error> {
        trace!(
            "Creating country association between film_id={} and country_id={}",
            film_id,
            country_id
        );

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
    pub fn create_film_year(&self, film_id: u64, year: &FilmYear) -> Result<(), Error> {
        trace!(
            "Creating film year for film_id={}, year={:?}",
            film_id,
            year
        );

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
    pub fn create_film_status(
        &self,
        film_id: u64,
        status: &GetFilmResponseStatus,
    ) -> Result<(), Error> {
        trace!(
            "Creating film status for film_id={}, status={:?}",
            film_id,
            status
        );

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
    pub fn create_film_competition(&self, film_id: u64, competition: &str) -> Result<(), Error> {
        trace!(
            "Creating film competition {} for film_id={}",
            competition,
            film_id,
        );

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
                    "Creating new genre (id={}, title={})",
                    genre.id,
                    genre.title
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
                stmt.query_row([country.code()], |row| row.get(0))
            })
            .filter_map(Result::ok)
            .collect();

        for country in countries.iter() {
            if !existing_countries.iter().any(|x| x == &country.code) {
                trace!(
                    "Creating new country (title={}, code={})",
                    country.title,
                    country.code
                );

                self.create_country(country.title(), country.code())?;
            }
        }

        Ok(())
    }
}

impl Deref for Database {
    type Target = Connection;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
