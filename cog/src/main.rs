use anyhow::Result;
use std::path::{Path, PathBuf};

use cog::config::Config;
use cog::organise::organise_by_tag;

fn process_vault(vault: &Path) -> Result<()> {
    tracing::info!("Processing vault: {}", vault.display());

    organise_by_tag(vault, "game", Some("games"))?;

    tracing::info!("Finished processing.");
    Ok(())
}

fn main() -> Result<()> {
    tracing_subscriber::fmt().without_time().init();

    let conf = Config::load()?;
    let vault = PathBuf::from(conf.vault);

    process_vault(&vault)?;

    Ok(())
}
