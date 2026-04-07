import logging

import requests

logger = logging.getLogger(__name__)


def player_data(id: int, key: str) -> None:
    logger.info("Starting player data")
    url = f"http://api.steampowered.com/ISteamUser/GetPlayerSummaries/v0002/?key={key}&steamids={id}&format=json"
    logger.debug(url)
    api = requests.get(url)
    print(api.json())

    url2 = f"http://api.steampowered.com/IPlayerService/GetOwnedGames/v0001/?key={key}&steamid={id}&format=json"

    api = requests.get(url2)
    print(api.json())
