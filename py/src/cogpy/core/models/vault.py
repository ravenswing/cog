import logging
from pathlib import Path
from typing import Iterator

from pydantic import BaseModel, DirectoryPath
from rich import print

from ..constants import EXCL_DIRS
from ..sync import get_last_sync, update_last_sync
from .notes import Note

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
        _sync_unidirectional(src=self, dest=right, back_update=back_update)


def _sync_unidirectional(src: Vault, dest: Vault, back_update: bool = False) -> None:
    src_notes = src.get_relative_paths()
    dest_notes = dest.get_relative_paths()

    # TODO! move this out side or into config
    exclude_dirs = ["meetings", "projects", "people"]
    exclude_tags = ["nbd", "no-sync", "client"]

    # Filter out excluded directories:
    src_notes = [p for p in src_notes if p.parent.name not in exclude_dirs]

    # Notes in the source vault with no counterparts in the destination = just copy into
    notes_to_copy = set(src_notes) - set(dest_notes)
    for p in notes_to_copy:
        src_note = Note.from_md(src.path / p)
        # Skip (verbosely) files with excluded tags
        if any(t in exclude_tags for t in src_note.tags):
            logger.info(
                f'SKIPPING note "{src_note.path.stem}" because of tags: {src_note.tags}'
            )
            continue

        print(f"[bold green]Creating[/bold green] new file {src_note.path}")
        src_note.path.copy(dest.path / p, preserve_metadata=True)

    overlap = set(src_notes) & set(dest_notes)
    conflicts = []
    last_sync = get_last_sync()
    for p in overlap:
        src_note = Note.from_md(src.path / p)
        dest_note = Note.from_md(dest.path / p)
        if any(t in exclude_tags for t in src_note.tags):
            logger.info(
                f'SKIPPING note ""{src_note.path.stem}" because of tags: {src_note.tags}'
            )
            continue

        match src_note.mtime, dest_note.mtime:
            # Both notes not updated since last sync = no changes to sync
            case l, r if l < last_sync and r < last_sync:
                logger.info(f'NO CHANGES - skipping "{p}"')
                continue
            # Both notes updated = potential conflict, save to report later
            case l, r if l > last_sync and r > last_sync:
                conflicts.append(f"{src_note.path}  <->  {dest_note.path}")
                continue
            # Only destination note modified = only change if back-updating
            case l, r if l < r:
                if back_update:
                    print(
                        f'[bold orange]Back-updating[/bold orange] - "{dest_note.path}" -> "{src_note.path}"'
                    )
                    dest_note.path.copy(src.path / p, preserve_metadata=True)
                else:
                    logger.info(f'Right modified after left - skipping "{p}"')
                    continue
            # Source modified = update
            case l, r if l > r:
                print(
                    f'[bold bright_green]Syncing[/bold bright_green] - "{src_note.path}" -> "{dest_note.path}"'
                )
                src_note.path.copy(dest.path / p, preserve_metadata=True)
            case _:
                raise ValueError("Unexpected time pattern")
    # Collates the paths for which there was a "conflict" - both updates since sync.
    if conflicts:
        print(
            "[bold bright_red]WARNING[/bold bright_red] - conflicts detected, check the log file!"
        )
        logger.warning(
            f"Conflicts were found - both files were updated since last sync: {'\n'.join(conflicts)}"
        )
    # Update the saved time for the last sync
    update_last_sync()
