use std::path::PathBuf;

use super::client::{Client, OwnedGame};
use super::error::{Error, Result};
use crate::config::SteamConfig;
use crate::fsutil::write_atomic;

/// Files written by [`fetch_player_data`].
#[derive(Debug)]
pub struct Fetched {
    pub summary: PathBuf,
    pub owned_games: PathBuf,
    pub game_count: u32,
}

/// Downloads the player summary and owned games into `data_path`, as
/// `player_summary.json` and `owned_games.csv`.
pub fn fetch_player_data(config: &SteamConfig) -> Result<Fetched> {
    let client = Client::from_key_file(&config.api_file)?;

    let mut summary = client.player_summary(config.id)?;
    let owned = client.owned_games(config.id)?;
    summary.game_count = Some(owned.count);

    let summary_path = config.data_path.join("player_summary.json");
    let json = serde_json::to_vec(&summary).map_err(|e| Error::Encode {
        what: "summary",
        source: e.into(),
    })?;
    write_atomic(&summary_path, &json)?;

    let games_path = config.data_path.join("owned_games.csv");
    write_atomic(&games_path, &games_csv(&owned.games)?)?;

    Ok(Fetched {
        summary: summary_path,
        owned_games: games_path,
        game_count: owned.count,
    })
}

fn games_csv(games: &[OwnedGame]) -> Result<Vec<u8>> {
    let encode = |source| Error::Encode {
        what: "owned games",
        source,
    };
    let mut writer = csv::Writer::from_writer(Vec::new());
    for game in games {
        writer.serialize(game).map_err(|e| encode(e.into()))?;
    }
    writer
        .into_inner()
        .map_err(|e| encode(e.into_error().into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_csv_with_blanks_for_missing_fields() {
        let games = [
            OwnedGame {
                appid: 10,
                playtime_forever: Some(5),
                rtime_last_played: Some(1700000000),
            },
            OwnedGame {
                appid: 20,
                playtime_forever: Some(0),
                rtime_last_played: None,
            },
        ];
        let csv = String::from_utf8(games_csv(&games).unwrap()).unwrap();
        assert_eq!(
            csv,
            "appid,playtime_forever,rtime_last_played\n10,5,1700000000\n20,0,\n"
        );
    }
}
