from os import getenv
from pathlib import Path

MOD_PATH = Path(__file__).parent.parent

# Same locations as the Rust version, so both share one config and last sync
CONFIG_PATH = Path(
    getenv("COG_CONFIG_PATH")
    or Path(getenv("XDG_CONFIG_HOME") or "~/.config") / "cog/cog.toml"
).expanduser()

INFO_PATH = Path(
    getenv("COG_INFO_PATH")
    or Path(getenv("XDG_STATE_HOME") or "~/.local/state") / "cog/state.toml"
).expanduser()

EXCL_DIRS = {"templates", "bases"}
