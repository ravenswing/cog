import logging
import logging.config
from glob import glob
from pathlib import Path
from time import time
from types import SimpleNamespace

from toml import dump as toml_dump
from toml import load as toml_load
from yaml import safe_load

from src import steam
from src.classes import Note

with open("src/logging.yaml", "r") as f:
    config = safe_load(f)

logging.config.dictConfig(config)
logger = logging.getLogger(__name__)

logger.info("Info message from main")
logger.info("IN BOTH FILES")
logger.debug(Path.cwd())


def find_tagged_notes(tag: str, exact: bool = True) -> list[Note]:
    tagged_notes = []
    for file in glob(f"{CONF.vault}/**/*.md") + glob(f"{CONF.vault}/*.md"):
        if f"{CONF.vault}/templates/" in file:
            continue

        note = Note(file)

        if exact and note.tags:
            if tag in note.tags:
                tagged_notes.append(note)
        elif note.tags:
            if any([tag in x for x in note.tags]):
                tagged_notes.append(note)
        else:
            continue

    return tagged_notes


def sync_note(note_path1, note_path2, last_sync_time):
    # Load both notes into Note class
    note1 = Note(note_path1)
    note2 = Note(note_path2)
    # Confirm that the names are the same
    assert (
        note1.name == note2.name
    ), f"Note names do not match {note1.name} =/= {note2.name}"
    # Confirm that the time modified has been found for both notes
    assert (
        note1._mtime is not None and note2._mtime is not None
    ), "Need to know time modified to sync."

    # If both notes were not edited since the last sync -> do nothing
    if note1._mtime < last_sync_time and note2._mtime < last_sync_time:
        return

    # If both notes WERE edited since the last sync -> WARN and do nothing (yet)
    elif note1._mtime > last_sync_time and note2._mtime > last_sync_time:
        print(
            f" MERGE -> Note: | {note1.name:<50} | was edited in both vaults since last sync!"
        )
        return

    # If Note1 was modified after Note2, update Note2
    elif note1._mtime > note2._mtime:
        note2.content = note1.content
        note2.save()
    # IF Note2 was modified after Note1, update Note1
    elif note2._mtime > note1._mtime:
        note1.content = note2.content
        note1.save()

    else:
        print(f"Unexpected time conf. with notes:\n  {note1.path}\n  {note2.path}")


def intersync_vaults(vault1: str, vault2: str, last_sync_time):
    # Check that there are no notes with the same name (in dif. folders)
    for v in [vault1, vault2]:
        check_names = [
            p.split("/")[-1][:-3] for p in glob(f"{v}/**/*.md") + glob(f"{v}/*.md")
        ]

        assert len(check_names) == len(
            set(check_names)
        ), f"Found duplicate names in {v}"

    # Load notes in each vault, with the name as the key
    vault1_notes = {
        p.split("/")[-1][:-3]: p
        for p in glob(f"{vault1}/**/*.md") + glob(f"{vault1}/*.md")
    }
    vault2_notes = {
        p.split("/")[-1][:-3]: p
        for p in glob(f"{vault2}/**/*.md") + glob(f"{vault2}/*.md")
    }

    # Identify ONLY notes that are in both vaults for syncing
    to_sync = set(vault1_notes.keys()) & set(vault2_notes.keys())

    for note in to_sync:
        print(note)
        sync_note(
            vault1_notes[note],
            vault2_notes[note],
            last_sync_time,
        )


def main():
    global CONF, info
    with open(f"{Path.home()}/.config/rhg/rhg.toml", "r") as f:
        CONF = SimpleNamespace(**toml_load(f))

    # Steam Web API key to access the data
    global KEY
    key_file = Path(CONF.api_file)
    KEY = key_file.read_text().strip()

    info_path = f"{Path(__file__).resolve().parent}/info.toml"

    with open(info_path, "r") as f:
        info_dict = toml_load(f)
    info = SimpleNamespace(**info_dict)

    logger.error("finished auxiliary_module.Auxiliary.do_something")
    logger.warning("calling auxiliary_module.some_function()")
    logger.debug("done with auxiliary_module.some_function()")

    logger.debug("only in debug")

    steam.player_data(CONF.steam_id, key=KEY)

    # game_notes = find_tagged_notes("pandas", exact=False)

    # for note in game_notes:
    # print(note.name)
    #
    # print(len(find_tagged_notes("programming/python")))
    # print(len(find_tagged_notes("python", exact=False)))

    # note = Note(f"{conf.vault}/python - structing a project.md")
    # print(note)
    # note.path = f"{conf.vault}/test test.md"
    # print("\n\n")
    # note.save()
    """
    intersync_vaults(
        conf.vault,
        conf.work_vault,
        info.last_sync,
    )
    """

    print(time())

    # Write the modified config back to the file
    info_dict["last_sync"] = time()
    with open(info_path, "w") as f:
        toml_dump(info_dict, f)


if __name__ == "__main__":
    main()
