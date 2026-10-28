use std::path::PathBuf;

/// Everything that can go wrong talking to Steam or saving what it returns.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("failed to read the Steam API key from {path}")]
    Key {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// The request URL is stripped, as it contains the API key.
    #[error("Steam API request failed")]
    Http(#[source] reqwest::Error),

    #[error("Steam returned no {0}, is the profile public?")]
    Missing(&'static str),

    #[error("failed to encode {what}")]
    Encode {
        what: &'static str,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    #[error(transparent)]
    Write(#[from] crate::Error),
}

impl From<reqwest::Error> for Error {
    fn from(e: reqwest::Error) -> Self {
        Self::Http(e.without_url())
    }
}

pub type Result<T, E = Error> = std::result::Result<T, E>;
