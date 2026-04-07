import os
from dataclasses import dataclass, field
from re import findall

from yaml import dump, safe_load


def read_in_tags(lines: list[str]):
    # Find all instances of a single # followed by a word in note content
    tags = [findall("#{1}[^#\s][a-z/]+\s", x) for x in lines]
    # Flatten list and strip whitespace and # from all hits
    tags = [item.strip()[1:] for sublist in tags for item in sublist if sublist]
    return tags


@dataclass(slots=True)
class Note:
    path: str
    name: str | None = None
    content: list[str] | None = None
    properties: dict | None = None
    tags: list[str] | None = None
    _mtime: float | None = field(init=False, repr=False)

    def __post_init__(self) -> None:
        if any([x is None for x in [self.name, self.content]]):
            assert os.path.exists(self.path), (
                "Must provide name and content for non-existant Note."
            )
            # Get the note name from the file path
            self.name = self.path.split("/")[-1][:-3]
            # Get the last-modified time
            self._mtime = os.path.getmtime(self.path)

            with open(self.path, "r") as f:
                lines = f.readlines()

            if not lines:
                self.content = []
            elif lines[0].strip() == "---":
                indexes = [i for i, value in enumerate(lines) if value.strip() == "---"]
                self.properties = safe_load("".join(lines[indexes[0] + 1 : indexes[1]]))
                self.content = lines[indexes[1] + 1 :]
            else:
                self.content = lines

            tags = []
            if self.properties and "tags" in self.properties.keys():
                yaml_tags = self.properties.pop("tags")
                tags.extend(yaml_tags)
            if self.content:
                tags.extend(read_in_tags(self.content))
            self.tags = list(set(tags))

    def save(self, overwrite=True) -> None:
        if not overwrite:
            assert not os.path.exists(self.path), (
                f"Overwrite is set off -> {self.name} already exists"
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

        text = []
        if self.properties:
            text.append("---\n")
            text.extend(dump(self.properties))
            text.append("---\n")

        if self.content:
            text.extend(self.content)

        with open(self.path, "w") as f:
            f.writelines(text)
