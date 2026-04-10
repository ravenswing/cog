import logging
from pathlib import Path
from typing import Any, Self

from pydantic import BaseModel, FilePath
from yaml import dump, safe_load

from .._utils import get_tags
from ..types import GameStatus, SteamReviews

logger = logging.getLogger(__name__)


class Note(BaseModel):
    path: FilePath
    title: str
    tags: set
    body: list[str]
    mtime: float
    properties: dict = {}

    @classmethod
    def from_md(cls, path: Path) -> Self:
        path = path.expanduser().resolve()

        # Create the note dict with path and time modified
        note: dict[str, Any] = {"path": path, "mtime": path.stat().st_mtime}

        # Read the contents of the file
        lines = path.read_text().split("\n")
        # Set the title based on the first H1 or the name of the file
        note["title"] = next((a[2:] for a in lines[:10] if "# " in a), path.stem)

        # Extract and read the properties YAML from the body of the note
        note["properties"] = {}
        if not lines:
            note["body"] = ["\n"]
        elif lines[0].strip() == "---":
            indexes = [i for i, line in enumerate(lines) if line.strip() == "---"]
            note["properties"] = safe_load(
                "\n".join(lines[indexes[0] + 1 : indexes[1]])
            )
            # Make certain that tags are a list if coming from the properties!
            if note["properties"].get("tags") and isinstance(
                note["properties"].get("tags"), str
            ):
                note["properties"]["tags"] = [note["properties"].get("tags")]
            note["body"] = lines[indexes[1] + 1 :]
        else:
            note["body"] = lines

        # Set the tags based on the body and properties
        note["tags"] = get_tags(note["body"], note["properties"])

        # Return the constructed Note
        return cls(**note)

    def save(self, overwrite: bool = True) -> None:
        if not overwrite:
            assert not self.path.exists(), (
                f"Overwrite is set to off -> {self.path.name} already exists"
            )

        if self.tags and self.properties:
            if "tags" in self.properties.keys():
                self.properties["tags"] = list(
                    set(self.tags) | set(self.properties["tags"])
                )
            else:
                self.properties["tags"] = self.tags
        elif self.tags:
            self.properties = {"tags": self.tags}

        text: list[str] = []
        if self.properties:
            text.append("---\n")
            text.extend(dump(self.properties))
            text.append("---\n")
        if self.body:
            text.extend(self.body)

        self.path.write_text("\n".join((text)))


class GameNote(Note):
    status: GameStatus
    playtime: float
    score: float
    steam_tags: set
    steam_reviews: SteamReviews
