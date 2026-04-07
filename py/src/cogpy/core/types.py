from enum import Enum, auto

from pydantic import BaseModel


class GameStatus(Enum):
    """Holds status of game within a game library"""

    Wishlist = auto()
    Untouched = auto()
    Tried = auto()
    Played = auto()
    Completed = auto()


class SteamRating(Enum):
    """All possible rating categories for a Steam game"""

    OverPos = "Overwhelmingly Positive"
    VeryPos = "Very Positive"
    Pos = "Positive"
    MostlyPos = "Mostly Positive"
    Mixed = "Mixed"
    MostlyNeg = "Mostly Negative"
    Neg = "Negative"
    VeryNeg = "Very Negative"
    OverNeg = "Overwhelmingly Negative"


class SteamReviews(BaseModel):
    """Holds all data concerning the reviews of a game on Steam"""

    rating: SteamRating
    perc: float
    total_reviews: int
