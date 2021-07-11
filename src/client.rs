use std::{collections::HashMap, ops::Deref};

use reqwest::redirect::Policy;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use urlencoding::decode as url_decode;

use crate::Error;

const API_BASE_URI: &str = "https://api.offstream.dk";

/// Deserializes an instance of type `T` from a string of JSON text, while wrapping the error as
/// [`Error::JsonDeserializationFailed`].
#[inline]
fn json_from_str<'a, T>(s: &'a str) -> Result<T, Error>
where
    T: Deserialize<'a>,
{
    let jd = &mut serde_json::Deserializer::from_str(s);

    serde_path_to_error::deserialize(jd).map_err(Error::JsonDeserializationFailed)
}

/// Serialize the given data structure as a String of JSON, while wrapping any errors as
/// [`Error::JsonSerializationFailed`].
#[inline]
fn json_to_string<T>(value: &T) -> Result<String, Error>
where
    T: ?Sized + Serialize,
{
    serde_json::to_string(value).map_err(Error::JsonSerializationFailed)
}

#[derive(Debug)]
/// Client interface for offstream.dk
pub struct Client {
    /// The inner http client
    http: reqwest::Client,
    /// XSRF token needed to talk to the API.
    ///
    /// This should be set/updated by calling [`Client::update_xsrf_token`].
    xsrf_token: Option<String>,
}

/// The API response when loading film data
#[derive(Deserialize, Debug)]
struct GetFilmResponseRaw {
    /// The `data` field
    pub data: HashMap<String, Value>,

    /// The response `status`
    pub status: GetFilmResponseStatus,
}

/// The response from [`Client::get_film`]
#[derive(Deserialize, Debug)]
pub struct GetFilmResponse {
    #[serde(flatten)]
    pub data: GetFilmResponseData,

    pub status: GetFilmResponseStatus,
}

impl Deref for GetFilmResponse {
    type Target = GetFilmResponseData;

    fn deref(&self) -> &Self::Target {
        &self.data
    }
}

#[derive(Deserialize, Debug)]
pub struct FilmGenre {
    pub id: String,
    pub title: String,
}

impl FilmGenre {
    /// Returns the film genre's id.
    pub fn id(&self) -> &str {
        self.id.as_str()
    }

    /// Returns the film genre's title.
    pub fn title(&self) -> &str {
        self.title.as_str()
    }
}

#[derive(Deserialize, Debug)]
pub struct FilmCountry {
    pub title: String,
    pub code: String,
}

impl FilmCountry {
    pub fn title(&self) -> &str {
        self.title.as_str()
    }

    pub fn code(&self) -> &str {
        self.code.as_str()
    }
}

#[derive(Deserialize, Debug)]
pub struct FilmYear {
    pub id: u64,
    pub title: Option<String>,
    pub product_id: Option<u64>,
}

#[derive(Deserialize, Debug)]
pub struct GetFilmResponseData {
    pub title: Option<String>,
    pub original_title: Option<String>,
    pub director: Option<String>,
    pub production_year: Option<u64>,
    pub duration: Option<u64>,
    pub description: Option<String>,
    pub age_restriction: Option<String>,
    pub thumbnails: HashMap<String, String>,
    pub genres: Vec<FilmGenre>,
    pub countries: Vec<FilmCountry>,
    pub year: FilmYear,
    pub competitions: Vec<String>,
}

#[derive(Deserialize, Debug)]
pub struct GetFilmResponseStatus {
    /// The general status of a response
    pub status: String,
    /// The vimeo video id, if any
    pub vimeo_id: Option<String>,
    /// The vimeo video id for the producers greeting video, if any
    pub greeting_vimeo_id: Option<String>,
}

impl Client {
    /// Returns a new client.
    ///
    /// # Errors
    ///
    /// If the [`reqwest::ClientBuilder`] fails to finalize, [`Error::HttpClientFailed`] is
    /// returned.
    pub fn new() -> Result<Client, Error> {
        let http_client = reqwest::Client::builder()
            .redirect(Policy::none())
            .cookie_store(true)
            .build()
            .map_err(Error::HttpClientFailed)?;

        let client = Client {
            http: http_client,
            xsrf_token: None,
        };

        Ok(client)
    }

    /// Requests a new XSRF token from the API, returning `Ok(())` on success.
    pub async fn update_xsrf_token(&mut self) -> Result<(), Error> {
        let res = self
            .build_get("/csrf-cookie")
            .send()
            .await
            .map_err(Error::XsrfTokenRequestFailed)?;

        // Extract the CSRF token
        if let Some(xsrf) = res
            .cookies()
            .find(|x| x.name() == "XSRF-TOKEN")
            .map(|x| x.value().to_owned())
        {
            self.xsrf_token = Some(url_decode(&xsrf)?);
        } else {
            return Err(Error::InvalidXsrfToken);
        }

        Ok(())
    }
    pub async fn get_film(&self, film_id: u64) -> Result<GetFilmResponse, Error> {
        let data = json_to_string(&json!({ "film_id": film_id }))?;
        let response = self.post("/films/load")?.body(data).send().await?;
        let raw_response: GetFilmResponseRaw = response.json().await?;

        if let Some(film_data) = raw_response.data.into_iter().next().map(|(_, value)| value) {
            Ok(GetFilmResponse {
                data: serde_path_to_error::deserialize(film_data)
                    .map_err(Error::JsonDeserializationFailed)?,
                status: raw_response.status,
            })
        } else {
            Err(Error::ApiError(
                "Could not deserialize film .data object".to_string(),
            ))
        }
    }

    /// Requests and returns a complete list of films.
    pub async fn get_films(&self) -> Result<serde_json::Value, Error> {
        let res = self.get("/films")?.send().await?;
        let body = res.text().await?;
        let json = json_from_str(&body)?;

        Ok(json)
    }

    /// Returns a new [`reqwest::RequestBuilder`] for a GET request with a set `x-xsrf-token` header and the
    /// given request `path`.
    ///
    /// # Errors
    ///
    /// Returns an error if [`Client::xsrf_token`] is `None`
    pub fn get(&self, path: &str) -> Result<reqwest::RequestBuilder, Error> {
        let xsrf_token = self.xsrf_token.as_ref().ok_or(Error::XsrfTokenMissing)?;
        let req = self.build_get(path).header("x-xsrf-token", xsrf_token);

        Ok(req)
    }

    #[inline]
    fn build_get(&self, path: &str) -> reqwest::RequestBuilder {
        self.http
            .get(&format!("{}{}", API_BASE_URI, path))
            .header("origin", "https://offstream.dk")
            .header("content-type", "application/json")
    }

    /// Returns a new [`reqwest::RequestBuilder`] for a POST request with a set `x-xsrf-token` header and the
    /// given request `path`.
    ///
    /// # Errors
    /// Returns an error if [`Client::xsrf_token`] is `None`
    pub fn post(&self, path: &str) -> Result<reqwest::RequestBuilder, Error> {
        let xsrf_token = self.xsrf_token.as_ref().ok_or(Error::XsrfTokenMissing)?;

        let req = self
            .http
            .post(&format!("{}{}", API_BASE_URI, path))
            .header("origin", "https://offstream.dk")
            .header("content-type", "application/json")
            .header("x-xsrf-token", xsrf_token);

        Ok(req)
    }
}
