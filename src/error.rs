use std::fmt;

use thiserror::Error;
use tracing_error::TracedError;

#[derive(Debug, Error)]
pub struct Error {
    source: TracedError<ErrorKind>,
}

impl fmt::Display for Error {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.source, fmt)
    }
}

impl<E> From<E> for Error
where
    ErrorKind: From<E>,
{
    fn from(source: E) -> Self {
        Self {
            source: ErrorKind::from(source).into(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ErrorKind {
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
