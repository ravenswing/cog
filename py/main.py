import logging
import logging.config
from pathlib import Path
from tomllib import load

with open("src/cogpy/utils/logging.toml", "rb") as f:
    config = load(f)
logging.config.dictConfig(config)
logger = logging.getLogger(__name__)

logger.info("Info message from main")
logger.info("IN BOTH FILES")
logger.debug(Path.cwd())


def main() -> None:
    logger.info("WORKING")


if __name__ == "__main__":
    main()
