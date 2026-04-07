from os import getenv
from pathlib import Path

MOD_PATH = Path(__file__).parent.parent

CONFIG_PATH = (
    Path(getenv("COG_CONFIG_PATH", "~/.config/cog/cog.toml")).expanduser().resolve()
)

if path := getenv("COG_INFO_PATH"):
    INFO_PATH = Path(path)
else:
    INFO_PATH = MOD_PATH.parent.parent / "info.toml"

EXCL_DIRS = {"templates", "bases"}
