use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Missing a valid XSRF token")]
    XsrfTokenMissing,
    #[error("Could not request a new XSRF token")]
    XsrfTokenRequestFailed(#[source] reqwest::Error),
    #[error("The API response did not include an XSRF token")]
    InvalidXsrfToken,
    #[error("URL decoding error")]
    UrlDecodeError(#[from] urlencoding::FromUrlEncodingError),
    #[error("JSON deserialization failed")]
    JsonDeserializationFailed(#[source] serde_path_to_error::Error<serde_json::Error>),
    #[error("JSON serialization failed")]
    JsonSerializationFailed(#[source] serde_json::Error),
    #[error("API error: {0}")]
    ApiError(String),
    #[error("Could not build reqwest HTTP client")]
    HttpClientFailed(#[source] reqwest::Error),
    #[error("HTTP request failed: {0}")]
    HttpRequestFailed(#[from] reqwest::Error),
    #[error("Database error: {0}")]
    DatabaseError(#[from] rusqlite::Error),
    #[error("Youtube-DL error: {0}")]
    YouTubeDlError(String),
}
