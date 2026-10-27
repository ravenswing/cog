# cog

_Cân O Garreg_ - _A Song of Stone_

Obsidian File Management & Analysis

<!-- TODO: add some background on what cog is for and how the vaults are used. -->

## Install

```sh
cargo install --path cog                    # vault tools only
cargo install --path cog --features steam   # with `cog steam`
```

## Config

cog reads `~/.config/cog/cog.toml`, or the file in `COG_CONFIG_PATH`, or `--config <path>`.

```toml
[vaults]
main = "/path/to/main/vault"
work = "/path/to/work/vault"

# Optional, these are the defaults
[sync]
skip_dirs = ["templates", "bases"]                # never walked, in either vault
exclude_dirs = ["meetings", "projects", "people"] # never synced from work
exclude_tags = ["no-sync", "client"]       # never synced from work

# Only needed for `cog steam`
[steam]
api_file = "steam_api.key"  # relative to this file
id = 123400000
data_path = "/path/to/data"
```

## Commands

- `cog sync` - syncs the work vault into the main vault.
  - Notes only in work are copied over.
  - Notes changed in only one vault since the last sync are copied to the other.
  - Notes changed in both are reported as conflicts and left alone.
  - `--dry-run` shows what would happen, `--no-back-update` stops changes in main being copied back to work.
- `cog title` - adds an H1 title from the filename to notes without one, dropping any `prefix - ` from the name. Defaults to the work vault.
- `cog organise <tag>` - moves notes in the root of the main vault with `<tag>` into a directory named after the tag, or `--dir <name>`.
- `cog steam fetch` - saves the Steam player summary and owned games to `steam.data_path`.

All commands take `-v` for more output and `-q` for less. `title` and `organise` also take `--dry-run` and `--vault main|work`.

## Files

- `~/.local/state/cog/state.toml` - time of the last sync, or the file in `COG_INFO_PATH`.
- `~/.local/state/cog/cog.log` - debug log of the last run.

## Python version

`py/` holds the original Python implementation, kept for benchmarking against the Rust version. It is no longer developed. It reads the same config and state files as the Rust version, so a sync by either moves the shared last sync time on.

## Data analysis

`data/` holds Python scripts for modelling the Steam datasets with scikit-learn. They are separate from cog and have no project file of their own yet.
