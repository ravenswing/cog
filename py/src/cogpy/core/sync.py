import logging
from pathlib import Path
from time import sleep, time
from tomllib import load

from tomli_w import dump

from .constants import INFO_PATH

logger = logging.getLogger(__name__)


def get_last_sync(toml: Path = INFO_PATH) -> float:
    """Return the last sync time, or 0.0 if nothing has synced yet."""
    if not toml.exists():
        return 0.0
    return load(toml.open("rb")).get("last_sync", 0.0)


def update_last_sync(timestamp: float = time(), toml: Path = INFO_PATH) -> None:
    """Record `timestamp` as the last sync time."""
    sleep(0.5)
    toml.parent.mkdir(parents=True, exist_ok=True)
    dump({"last_sync": timestamp}, toml.open("wb"))
