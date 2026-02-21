from pathlib import Path
from types import SimpleNamespace

from rich import print
from toml import load as toml_load


def add_inline_title(vault: Path) -> None:
    """Adds an H1 title based on the filename to files without titles."""
    # Iterate though all .md files
    for path in list(vault.rglob("*.md")):
        # Read the lines of the file
        note = path.read_text().strip().split("\n")

        # Find and hlines in the file
        hlines = [i for i, x in enumerate(note) if x == "---"]

        # Match hline pattern for properties header or not
        if hlines and len(hlines) > 1 and hlines[0] < 3:
            insert_at = hlines[1] + 1
        else:
            insert_at = 0

        # Detect if the file already has an H1 title at that location
        if len(note) > insert_at and note[insert_at].startswith("# "):
            print(
                f"[bright_yellow]NOTE:[/bright_yellow] {path.stem} already has title."
            )
            continue

        # Remove filename annotations
        if " - " in path.stem:
            title = " ".join(path.stem.split(" - ")[1:])
        else:
            title = path.stem

        # Capitalize the title
        title = title.capitalize()

        # Add the title line to the corrct point in the file
        note.insert(insert_at, f"# {title}")
        # Write the new file contents
        path.write_text("\n".join(note))


def main() -> None:
    with open(f"{Path.home()}/.config/rhg/rhg.toml", "r") as f:
        conf = SimpleNamespace(**toml_load(f))

    vault = Path(conf.work_vault)

    add_inline_title(vault)


if __name__ == "__main__":
    main()
