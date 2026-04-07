import logging
from pathlib import Path
from typing import Any, Iterator, Self

from pydantic import BaseModel, DirectoryPath, FilePath
from rich import print
from yaml import dump, safe_load

from ._utils import get_tags
from .constants import EXCL_DIRS
from .sync import get_last_sync, update_last_sync
from .types import GameStatus, SteamReviews

logger = logging.getLogger(__name__)


class Vault(BaseModel):
    path: DirectoryPath

    def iter_notes(self, exclude: set | None = EXCL_DIRS) -> Iterator[Note]:
        # Path.walk() yields (dirpath, dirnames, filenames)
        for root, dirs, files in self.path.walk():
            # Slice assignment modifies selection in place!
            if exclude:
                dirs[:] = [d for d in dirs if d not in exclude]

            # Yield the matching files in the current (non-excluded) directory
            for file in files:
                if file.endswith(".md"):
                    logger.debug(f"Loading: {root / file}")
                    yield Note.from_md(root / file)

    def get_relative_paths(self, exclude: set | None = EXCL_DIRS) -> list[Path]:
        rel_paths = []

        for root, dirs, files in self.path.walk():
            if exclude:
                dirs[:] = [d for d in dirs if d not in exclude]
            for file in files:
                if file.endswith(".md"):
                    rel_paths.append((root / file).relative_to(self.path))

        return rel_paths

    def sync_to(self, right: Vault, back_update: bool = False) -> None:
        sync_unidirectional(src=self, dest=right, back_update=back_update)


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


def sync_unidirectional(dest: Vault, src: Vault, back_update: bool = False) -> None:
    src_notes = src.get_relative_paths()
    dest_notes = dest.get_relative_paths()

    exclude_dirs = ["meetings", "projects", "people"]
    # Filter out excluded directories:
    src_notes = [p for p in src_notes if p.parent.name not in exclude_dirs]

    exclude_tags = ["nbd", "no-sync", "client"]
    # Notes in the left vault with no counterparts in the right
    notes_to_copy = set(src_notes) - set(dest_notes)
    for p in notes_to_copy:
        left_note = Note.from_md(src.path / p)
        # Skip (verbosely) files with excluded tags
        if any(t in exclude_tags for t in left_note.tags):
            logger.info(
                f'SKIPPING note "{left_note.path.stem}" because of tags: {left_note.tags}'
            )
            continue

        print(f"[bold green]Creating[/bold green] new file {left_note.path}")
        left_note.path.copy_into(dest.path)

    overlap = set(src_notes) & set(dest_notes)
    conflicts = []
    last_sync = get_last_sync()
    for p in overlap:
        left_note = Note.from_md(src.path / p)
        right_note = Note.from_md(dest.path / p)
        if any(t in exclude_tags for t in left_note.tags):
            logger.info(
                f'SKIPPING note ""{left_note.path.stem}" because of tags: {left_note.tags}'
            )
            continue

        match left_note.mtime, right_note.mtime:
            case l, r if l < last_sync and r < last_sync:
                logger.info(f'NO CHANGES - skipping "{p}"')
                continue
            case l, r if l > last_sync and r > last_sync:
                conflicts.append(f"{left_note.path}  <->  {right_note.path}")
                continue
            case l, r if l < r:
                logger.info(f'Right modified after left - skipping "{p}"')
                continue
            case l, r if l > r:
                print(
                    f'[bold bright_green]Syncing[/bold bright_green] - "{left_note.path}" -> "{right_note.path}"'
                )
                left_note.path.copy_into(dest.path)
            case _:
                raise ValueError("Unwelcome time pattern")

    if conflicts:
        print(
            "[bold bright_red]WARNING[/bold bright_red] - conflicts detected, check the log file!"
        )
        logger.warning(
            f"Conflicts were found - both files were updated since last sync: {'\n'.join(conflicts)}"
        )

    update_last_sync()
