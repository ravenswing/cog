from tomllib import load

from cogpy.core.constants import CONFIG_PATH


def load_config() -> dict:
    return load(CONFIG_PATH.open("rb"))
