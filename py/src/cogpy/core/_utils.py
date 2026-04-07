import logging
import re

logger = logging.getLogger(__name__)


def get_tags(lines: list[str], properties: dict) -> set:
    # Rejoin text for regex search, only first 15 lines
    text = " ".join(lines[:15])
    # Clean all link block and add space for regex to work
    cleaned_text = re.sub(r"\[\[.*?\]\]", "", text, flags=re.DOTALL) + " "
    # Find all instances of a single # followed by a word in note contents
    tags = re.findall("#{1}[^#\s][a-z/]+\s", cleaned_text)
    # Flatten list and strip whitespace and # from all hits
    tags = [tag.strip()[1:] for tag in tags]

    # Add all tags from the note properties
    if prop_tags := properties.get("tags"):
        logger.debug(type(prop_tags))
        tags.extend(prop_tags)
    # Expand nested tags for usability and ease
    for tag in tags:
        if "/" in tag:
            tags.extend(tag.split("/"))
    # Return a set for quick look up and to remove any duplicates
    unique_tags = set(tags)
    logger.debug(f"{unique_tags}")
    return unique_tags
