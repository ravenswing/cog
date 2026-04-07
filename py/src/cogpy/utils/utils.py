from pathlib import Path
from time import time
from tomllib import load

from tomli_w import dump

from cogpy.core.constants import CONFIG_PATH, INFO_PATH


def load_config() -> dict:
    return load(CONFIG_PATH.open("rb"))
