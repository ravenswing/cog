use anyhow::{Context, Result};
use serde::Deserialize;
use std::fs;
use std::path::PathBuf;

/// Matches the expected structure of the TOML config file.
#[derive(Debug, Deserialize)]
pub struct Config {
    pub vault: String,
    pub work_vault: String,
}

impl Config {
    /// Reads the config from `~/.config/rhg.toml`.
    pub fn load() -> Result<Self> {
        let home_dir = dirs::home_dir().context("Could not find the home directory")?;
        let config_path: PathBuf = home_dir.join(".config/rhg.toml");

        let contents = fs::read_to_string(&config_path)
            .with_context(|| format!("Failed to read config file: {}", config_path.display()))?;

        toml::from_str(&contents)
            .context("Failed to parse TOML configuration. Please check your syntax.")
    }
}
