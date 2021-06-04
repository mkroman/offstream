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
    film_id INTEGER UNIQUE REFERENCES films (id) ON DELETE CASCADE,
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
    film_id INTEGER UNIQUE REFERENCES films (id) ON DELETE CASCADE,
    title VARCHAR,
    product_id VARCHAR
);

CREATE TABLE IF NOT EXISTS film_downloads (
    id INTEGER PRIMARY KEY,
    film_id INTEGER UNIQUE REFERENCES films (id) ON DELETE CASCADE,
    started_at DATETIME DEFAULT CURRENT_TIMESTAMP NOT NULL,
    finished_at DATETIME,
    path VARCHAR NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_film_thumbnails ON film_thumbnails (film_id);
CREATE UNIQUE INDEX IF NOT EXISTS idx_genres ON genres (identifier);
CREATE UNIQUE INDEX IF NOT EXISTS idx_film_genres ON film_genres (film_id, genre_id);
CREATE UNIQUE INDEX IF NOT EXISTS idx_film_countries ON film_countries (film_id, country_id);
CREATE INDEX IF NOT EXISTS idx_film_genres ON film_genres (film_id, genre_id);
CREATE INDEX IF NOT EXISTS idx_film_downloads ON film_downloads (film_id);

COMMIT;
