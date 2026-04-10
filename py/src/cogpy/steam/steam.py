import json
import logging

import polars as pl
import requests
from rich import print

from cogpy.core.constants import MOD_PATH

logger = logging.getLogger(__name__)
CHROME_HEADERS = {
    "User-Agent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/114.0.0.0 Safari/537.36"
}


def player_data(id: int, key: str) -> None:
    logger.info("Starting player data")
    data_dir = MOD_PATH.parent.parent.parent / "data"

    summary_url = f"http://api.steampowered.com/ISteamUser/GetPlayerSummaries/v0002/?key={key}&steamids={id}&format=json"
    logger.debug(summary_url)
    print(summary_url)
    api = requests.get(summary_url)

    summary = json.loads(api.text)["response"]["players"][0]
    print(summary)

    # Owned games
    owned_games_url = f"http://api.steampowered.com/IPlayerService/GetOwnedGames/v0001/?key={key}&steamid={id}&format=json"
    logger.debug(owned_games_url)
    print(owned_games_url)

    api = requests.get(owned_games_url)
    owned_json = json.loads(api.text)["response"]
    games_count = owned_json["game_count"]

    summary["game_count"] = games_count
    summary_path = data_dir / "player_summary.json"
    with summary_path.open("w") as fp:
        json.dump(summary, fp)

    owned_games = pl.from_dicts(owned_json["games"]).select(
        "appid", "playtime_forever", "rtime_last_played"
    )
    print(owned_games)
    owned_games.write_csv(data_dir / "owned_games.csv")
