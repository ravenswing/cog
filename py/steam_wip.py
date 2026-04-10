import logging
import logging.config
from tomllib import load

from rich import print

from cogpy.core.constants import CONFIG_PATH
from cogpy.steam import player_data
from cogpy.utils import load_config

log_config = load(open("src/cogpy/utils/logging.toml", "rb"))
logging.config.dictConfig(log_config)
logger = logging.getLogger(__name__)


def main() -> None:
    config = load_config()
    api_key = (
        CONFIG_PATH.parent.joinpath(config["steam"]["api_file"])
        .resolve()
        .read_text()
        .strip()
    )
    player_data(config["steam"]["id"], api_key)


if __name__ == "__main__":
    main()
