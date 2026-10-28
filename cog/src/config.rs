use serde::Deserialize;
use std::collections::BTreeSet;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use crate::error::{Error, IoContext, Result};
use crate::tags::Tag;

/// Environment variable that overrides the config file location.
pub const CONFIG_ENV: &str = "COG_CONFIG_PATH";

/// The contents of `cog.toml`.
#[derive(Debug, Deserialize)]
pub struct Config {
    pub vaults: Vaults,
    #[serde(default)]
    pub sync: SyncConfig,
    pub steam: Option<SteamConfig>,
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
    pub skip_dirs: BTreeSet<String>,
    /// Directories in the source vault whose notes are never synced.
    pub exclude_dirs: BTreeSet<String>,
    /// Tags that stop a source note from being synced.
    pub exclude_tags: BTreeSet<Tag>,
}

/// Steam account details for `cog steam`.
#[derive(Debug, Deserialize)]
pub struct SteamConfig {
    /// File holding the Steam Web API key. Relative paths are resolved
    /// against the config file's directory.
    pub api_file: PathBuf,
    /// The 64-bit Steam ID of the account.
    pub id: u64,
    /// Directory that downloaded data is written to.
    pub data_path: PathBuf,
}

fn owned(names: &[&str]) -> BTreeSet<String> {
    names.iter().map(|s| s.to_string()).collect()
}

impl Default for SyncConfig {
    fn default() -> Self {
        Self {
            skip_dirs: owned(&["templates", "bases"]),
            exclude_dirs: owned(&["meetings", "projects", "people"]),
            exclude_tags: ["nbd", "no-sync", "client"]
                .into_iter()
                .filter_map(Tag::parse)
                .collect(),
        }
    }
}

impl Config {
    /// Path from `COG_CONFIG_PATH`, else `<config dir>/cog/cog.toml`.
    #[must_use]
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
        let mut config: Self = toml::from_str(&contents).map_err(|source| Error::Config {
            path: path.to_owned(),
            source,
        })?;
        if let (Some(steam), Some(dir)) = (&mut config.steam, path.parent()) {
            steam.api_file = dir.join(&steam.api_file);
        }
        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tags(raw: &[&str]) -> BTreeSet<Tag> {
        raw.iter().filter_map(|t| Tag::parse(t)).collect()
    }

    #[test]
    fn sync_section_defaults_when_missing() {
        let config: Config = toml::from_str("[vaults]\nmain = '/m'\nwork = '/w'").unwrap();
        assert_eq!(
            config.sync.exclude_tags,
            tags(&["nbd", "no-sync", "client"])
        );
        assert_eq!(config.sync.skip_dirs, owned(&["templates", "bases"]));
    }

    #[test]
    fn partial_sync_section_keeps_other_defaults() {
        let config: Config = toml::from_str(
            "[vaults]\nmain = '/m'\nwork = '/w'\n[sync]\nexclude_tags = ['private']",
        )
        .unwrap();
        assert_eq!(config.sync.exclude_tags, tags(&["private"]));
        assert_eq!(
            config.sync.exclude_dirs,
            owned(&["meetings", "projects", "people"])
        );
    }

    #[test]
    fn normalises_and_validates_exclude_tags() {
        let vaults = "[vaults]\nmain = '/m'\nwork = '/w'\n";
        let config: Config =
            toml::from_str(&format!("{vaults}[sync]\nexclude_tags = ['#Client']")).unwrap();
        assert_eq!(config.sync.exclude_tags, tags(&["client"]));
        let err = toml::from_str::<Config>(&format!(
            "{vaults}[sync]
exclude_tags = ['2024']"
        ))
        .unwrap_err();
        assert!(err.to_string().contains("not a valid tag"), "{err}");
    }

    #[test]
    fn ignores_unknown_sections() {
        let config: Config =
            toml::from_str("[vaults]\nmain = '/m'\nwork = '/w'\n[other]\nid = 1").unwrap();
        assert_eq!(config.vaults.main, Path::new("/m"));
        assert!(config.steam.is_none());
    }

    #[test]
    fn resolves_api_file_against_config_dir() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cog.toml");
        fs::write(
            &path,
            "[vaults]\nmain = '/m'\nwork = '/w'\n\
             [steam]\napi_file = 'key'\nid = 1\ndata_path = '/d'\nusername = 'x'",
        )
        .unwrap();
        let steam = Config::load(&path).unwrap().steam.unwrap();
        assert_eq!(steam.api_file, dir.path().join("key"));
    }
}
