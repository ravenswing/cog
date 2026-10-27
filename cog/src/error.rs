use std::io;
use std::path::PathBuf;

/// Everything that can go wrong inside the cog library.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("failed to {action} {path}")]
    Io {
        action: &'static str,
        path: PathBuf,
        #[source]
        source: io::Error,
    },

    #[error("invalid config in {path}")]
    Config {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },

    #[error("invalid state file {path}")]
    State {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },

    #[error("invalid last sync time {value} in {path}")]
    LastSync { path: PathBuf, value: f64 },

    #[error("failed to encode the state file")]
    StateEncode(#[source] toml::ser::Error),

    #[error("the system clock is set before 1970")]
    Clock(#[source] std::time::SystemTimeError),

    #[error("frontmatter in {path} has no closing `---`")]
    UnclosedFrontmatter { path: PathBuf },

    #[error("invalid YAML frontmatter in {path}")]
    Yaml {
        path: PathBuf,
        #[source]
        source: serde_norway::Error,
    },

    #[error("{path} is not a directory")]
    NotADirectory { path: PathBuf },

    #[error("failed to walk vault")]
    Walk(#[from] walkdir::Error),
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Attaches an action and path to an [`io::Error`].
pub(crate) trait IoContext<T> {
    fn ctx(self, action: &'static str, path: impl Into<PathBuf>) -> Result<T>;
}

impl<T> IoContext<T> for io::Result<T> {
    fn ctx(self, action: &'static str, path: impl Into<PathBuf>) -> Result<T> {
        self.map_err(|source| Error::Io {
            action,
            path: path.into(),
            source,
        })
    }
}
