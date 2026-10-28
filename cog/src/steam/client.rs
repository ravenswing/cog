use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::fs;
use std::path::Path;

use super::error::{Error, Result};

const API: &str = "https://api.steampowered.com";

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

/// Every game an account owns.
#[derive(Debug)]
pub struct OwnedGames {
    /// The total Steam reports, which can differ from `games.len()`.
    pub count: u32,
    pub games: Vec<OwnedGame>,
}

#[derive(Deserialize)]
struct Response<T> {
    response: T,
}

#[derive(Deserialize)]
struct Players {
    players: Vec<PlayerSummary>,
}

/// `GetOwnedGames` as Steam sends it, with nothing for a private profile.
#[derive(Deserialize)]
struct OwnedGamesResponse {
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

    /// Fetches the profile for `steam_id`. Fails with [`Error::Missing`] if
    /// Steam returns no player.
    pub fn player_summary(&self, steam_id: u64) -> Result<PlayerSummary> {
        let players: Players =
            self.get("ISteamUser/GetPlayerSummaries/v0002/", "steamids", steam_id)?;
        players
            .players
            .into_iter()
            .next()
            .ok_or(Error::Missing("player summary"))
    }

    /// Fetches every game `steam_id` owns. Fails with [`Error::Missing`] if
    /// the profile's game details are private.
    pub fn owned_games(&self, steam_id: u64) -> Result<OwnedGames> {
        let owned: OwnedGamesResponse =
            self.get("IPlayerService/GetOwnedGames/v0001/", "steamid", steam_id)?;
        let count = owned.game_count.ok_or(Error::Missing("owned games"))?;
        Ok(OwnedGames {
            count,
            games: owned.games,
        })
    }
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
        let response: Response<OwnedGamesResponse> =
            serde_json::from_str(r#"{"response":{}}"#).unwrap();
        assert!(response.response.game_count.is_none());
    }
}
