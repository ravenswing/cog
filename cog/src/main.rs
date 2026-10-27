use anyhow::Result;
use std::path::Path;

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

    let conf = Config::load(&Config::default_path())?;

    process_vault(&conf.vaults.main)?;

    Ok(())
}
