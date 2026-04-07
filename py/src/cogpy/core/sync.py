import logging
from pathlib import Path
from time import time
from tomllib import load

from tomli_w import dump

from .constants import INFO_PATH

logger = logging.getLogger(__name__)


def get_last_sync(toml: Path = INFO_PATH) -> float:
    return load(toml.open("rb")).get("last_sync", 0.0)


def update_last_sync(timestamp: float = time(), toml: Path = INFO_PATH) -> None:
    dump({"last_sync": timestamp}, toml.open("wb"))
