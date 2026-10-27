//! Downloading account data from the Steam Web API.

mod types;

pub use types::{GameStatus, SteamRating, SteamReviews};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::fs;
use std::path::{Path, PathBuf};

use crate::config::SteamConfig;
use crate::fsutil::write_atomic;

const API: &str = "https://api.steampowered.com";

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

    #[error("failed to encode {0}")]
    Encode(
        &'static str,
        #[source] Box<dyn std::error::Error + Send + Sync>,
    ),

    #[error(transparent)]
    Write(#[from] crate::Error),
}

impl From<reqwest::Error> for Error {
    fn from(e: reqwest::Error) -> Self {
        Self::Http(e.without_url())
    }
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

/// A player's public profile, as returned by `GetPlayerSummaries`.
#[derive(Debug, Serialize, Deserialize)]
pub struct PlayerSummary {
    pub steamid: String,
    pub personaname: String,
    /// Added from `GetOwnedGames`, Steam doesn't include it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub game_count: Option<u32>,
    /// Every other field Steam sends, kept as-is.
    #[serde(flatten)]
    pub other: Map<String, Value>,
}

/// One row of `owned_games.csv`.
#[derive(Debug, Serialize, Deserialize)]
pub struct OwnedGame {
    pub appid: u32,
    /// Minutes played in total.
    pub playtime_forever: Option<u32>,
    /// Unix time the game was last played.
    pub rtime_last_played: Option<u64>,
}

#[derive(Deserialize)]
struct Response<T> {
    response: T,
}

#[derive(Deserialize)]
struct Players {
    players: Vec<PlayerSummary>,
}

#[derive(Deserialize)]
struct OwnedGames {
    game_count: Option<u32>,
    #[serde(default)]
    games: Vec<OwnedGame>,
}

/// A Steam Web API client for one key.
pub struct Client {
    http: reqwest::blocking::Client,
    key: String,
}

impl Client {
    /// Builds a client using the key stored in `key_file`.
    pub fn from_key_file(key_file: &Path) -> Result<Self> {
        let key = fs::read_to_string(key_file).map_err(|source| Error::Key {
            path: key_file.to_owned(),
            source,
        })?;
        Ok(Self {
            http: reqwest::blocking::Client::new(),
            key: key.trim().to_owned(),
        })
    }

    /// Calls `endpoint`, passing the Steam ID as the query parameter `id_param`.
    fn get<T: for<'de> Deserialize<'de>>(
        &self,
        endpoint: &str,
        id_param: &str,
        steam_id: u64,
    ) -> Result<T> {
        let response: Response<T> = self
            .http
            .get(format!("{API}/{endpoint}"))
            .query(&[("key", self.key.as_str()), ("format", "json")])
            .query(&[(id_param, steam_id)])
            .send()?
            .error_for_status()?
            .json()?;
        Ok(response.response)
    }

    pub fn player_summary(&self, steam_id: u64) -> Result<PlayerSummary> {
        let players: Players =
            self.get("ISteamUser/GetPlayerSummaries/v0002/", "steamids", steam_id)?;
        players
            .players
            .into_iter()
            .next()
            .ok_or(Error::Missing("player summary"))
    }

    /// Returns the total number of games owned, and each game.
    pub fn owned_games(&self, steam_id: u64) -> Result<(u32, Vec<OwnedGame>)> {
        let owned: OwnedGames =
            self.get("IPlayerService/GetOwnedGames/v0001/", "steamid", steam_id)?;
        let count = owned.game_count.ok_or(Error::Missing("owned games"))?;
        Ok((count, owned.games))
    }
}

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
    let (game_count, games) = client.owned_games(config.id)?;
    summary.game_count = Some(game_count);

    let summary_path = config.data_path.join("player_summary.json");
    let json = serde_json::to_vec(&summary).map_err(|e| Error::Encode("summary", e.into()))?;
    write_atomic(&summary_path, &json)?;

    let games_path = config.data_path.join("owned_games.csv");
    write_atomic(&games_path, &games_csv(&games)?)?;

    Ok(Fetched {
        summary: summary_path,
        owned_games: games_path,
        game_count,
    })
}

fn games_csv(games: &[OwnedGame]) -> Result<Vec<u8>> {
    let encode = |e: csv::Error| Error::Encode("owned games", e.into());
    let mut writer = csv::Writer::from_writer(Vec::new());
    for game in games {
        writer.serialize(game).map_err(encode)?;
    }
    writer
        .into_inner()
        .map_err(|e| Error::Encode("owned games", e.into_error().into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summary_keeps_unknown_fields() {
        let json = r#"{"response":{"players":[
            {"steamid":"7656","personaname":"me","avatar":"a.png","timecreated":1}
        ]}}"#;
        let response: Response<Players> = serde_json::from_str(json).unwrap();
        let mut summary = response.response.players.into_iter().next().unwrap();
        summary.game_count = Some(3);

        let out: Value = serde_json::to_value(&summary).unwrap();
        assert_eq!(out["avatar"], "a.png");
        assert_eq!(out["timecreated"], 1);
        assert_eq!(out["game_count"], 3);
    }

    #[test]
    fn private_profile_has_no_games() {
        let response: Response<OwnedGames> = serde_json::from_str(r#"{"response":{}}"#).unwrap();
        assert!(response.response.game_count.is_none());
    }

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
