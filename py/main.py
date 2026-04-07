import logging
import logging.config
from tomllib import load

from rich import print

from cogpy.core.models import Vault
from cogpy.utils import load_config

log_config = load(open("src/cogpy/utils/logging.toml", "rb"))
logging.config.dictConfig(log_config)
logger = logging.getLogger(__name__)


def main() -> None:
    print("[bold bright_blue]Starting COG[/bold bright_blue]...")
    logger.info(" ========== Starting COG ========== ")

    config = load_config()

    main = Vault(path=config["vaults"]["main"])
    work = Vault(path=config["vaults"]["work"])

    work.sync_to(main)


if __name__ == "__main__":
    main()
