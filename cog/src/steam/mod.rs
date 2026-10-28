//! Downloading account data from the Steam Web API.

mod client;
mod error;
mod fetch;
mod types;

pub use client::{Client, OwnedGame, OwnedGames, PlayerSummary};
pub use error::{Error, Result};
pub use fetch::{Fetched, fetch_player_data};
pub use types::{GameStatus, SteamRating, SteamReviews};
