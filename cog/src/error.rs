use std::io;
use std::path::PathBuf;
use std::time::SystemTimeError;

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

    #[error("another cog run is in progress, it holds the lock on {path}")]
    Locked { path: PathBuf },

    #[error("failed to encode the state file")]
    StateEncode(#[source] toml::ser::Error),

    #[error("the system clock is set before 1970")]
    Clock(#[source] SystemTimeError),

    #[error("frontmatter in {path} has no closing `---`")]
    UnclosedFrontmatter { path: PathBuf },

    #[error("invalid YAML frontmatter in {path}")]
    Yaml {
        path: PathBuf,
        #[source]
        source: serde_norway::Error,
    },

    #[error("{path} has a name that isn't valid UTF-8")]
    NonUtf8Name { path: PathBuf },

    #[error("vaults {src} and {dest} are the same, or one is inside the other")]
    OverlappingVaults { src: PathBuf, dest: PathBuf },

    #[error("{path} is not a directory")]
    NotADirectory { path: PathBuf },

    #[error("failed to walk vault")]
    Walk(#[from] walkdir::Error),
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Attaches an action and path to an [`io::Error`].
pub(crate) trait IoResultExt<T> {
    fn ctx(self, action: &'static str, path: impl Into<PathBuf>) -> Result<T>;
}

impl<T> IoResultExt<T> for io::Result<T> {
    fn ctx(self, action: &'static str, path: impl Into<PathBuf>) -> Result<T> {
        self.map_err(|source| Error::Io {
            action,
            path: path.into(),
            source,
        })
    }
}
