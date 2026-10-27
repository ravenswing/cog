use serde::Deserialize;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use crate::error::{Error, IoContext, Result};

/// Environment variable that overrides the config file location.
pub const CONFIG_ENV: &str = "COG_CONFIG_PATH";

/// The contents of `cog.toml`.
#[derive(Debug, Deserialize)]
pub struct Config {
    pub vaults: Vaults,
    #[serde(default)]
    pub sync: SyncConfig,
}

#[derive(Debug, Deserialize)]
pub struct Vaults {
    pub main: PathBuf,
    pub work: PathBuf,
}

/// Rules for which notes are walked and synced.
#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct SyncConfig {
    /// Directories never walked, in either vault.
    pub skip_dirs: Vec<String>,
    /// Directories in the source vault whose notes are never synced.
    pub exclude_dirs: Vec<String>,
    /// Tags that stop a source note from being synced.
    pub exclude_tags: Vec<String>,
}

impl Default for SyncConfig {
    fn default() -> Self {
        let owned = |names: &[&str]| names.iter().map(|s| s.to_string()).collect();
        Self {
            skip_dirs: owned(&["templates", "bases"]),
            exclude_dirs: owned(&["meetings", "projects", "people"]),
            exclude_tags: owned(&["nbd", "no-sync", "client"]),
        }
    }
}

impl Config {
    /// Path from `COG_CONFIG_PATH`, else `<config dir>/cog/cog.toml`.
    pub fn default_path() -> PathBuf {
        match env::var_os(CONFIG_ENV) {
            Some(path) => PathBuf::from(path),
            None => dirs::config_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("cog/cog.toml"),
        }
    }

    /// Reads and parses the config at `path`.
    pub fn load(path: &Path) -> Result<Self> {
        let contents = fs::read_to_string(path).ctx("read config", path)?;
        toml::from_str(&contents).map_err(|source| Error::Config {
            path: path.to_owned(),
            source,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sync_section_defaults_when_missing() {
        let config: Config = toml::from_str("[vaults]\nmain = '/m'\nwork = '/w'").unwrap();
        assert_eq!(config.sync.exclude_tags, ["nbd", "no-sync", "client"]);
        assert_eq!(config.sync.skip_dirs, ["templates", "bases"]);
    }

    #[test]
    fn partial_sync_section_keeps_other_defaults() {
        let config: Config = toml::from_str(
            "[vaults]\nmain = '/m'\nwork = '/w'\n[sync]\nexclude_tags = ['private']",
        )
        .unwrap();
        assert_eq!(config.sync.exclude_tags, ["private"]);
        assert_eq!(config.sync.exclude_dirs, ["meetings", "projects", "people"]);
    }

    #[test]
    fn ignores_unknown_sections() {
        let config: Config =
            toml::from_str("[vaults]\nmain = '/m'\nwork = '/w'\n[steam]\nid = 1").unwrap();
        assert_eq!(config.vaults.main, Path::new("/m"));
    }
}
