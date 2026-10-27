use serde::{Deserialize, Serialize};

/// Status of a game within a game library.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GameStatus {
    Wishlist,
    Untouched,
    Tried,
    Played,
    Completed,
}

/// All possible rating categories for a Steam game.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SteamRating {
    #[serde(rename = "Overwhelmingly Positive")]
    OverwhelminglyPositive,
    #[serde(rename = "Very Positive")]
    VeryPositive,
    Positive,
    #[serde(rename = "Mostly Positive")]
    MostlyPositive,
    Mixed,
    #[serde(rename = "Mostly Negative")]
    MostlyNegative,
    Negative,
    #[serde(rename = "Very Negative")]
    VeryNegative,
    #[serde(rename = "Overwhelmingly Negative")]
    OverwhelminglyNegative,
}

/// All data concerning the reviews of a game on Steam.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SteamReviews {
    pub rating: SteamRating,
    /// Percentage of positive reviews.
    pub perc: f64,
    pub total_reviews: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rating_uses_steam_wording() {
        let rating: SteamRating = serde_json::from_str(r#""Mostly Positive""#).unwrap();
        assert_eq!(rating, SteamRating::MostlyPositive);
        assert_eq!(
            serde_json::to_string(&SteamRating::OverwhelminglyNegative).unwrap(),
            r#""Overwhelmingly Negative""#
        );
    }
}
