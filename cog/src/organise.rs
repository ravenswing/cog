//! Moving tagged notes into their own directory.

use rayon::prelude::*;
use std::ffi::OsStr;
use std::fmt;
use std::fs;
use std::path::Path;

use crate::error::{Error, IoContext, Result};
use crate::note::load_tags;
use crate::report::Report;
use crate::tags::Tag;
use crate::vault::{NotePath, Vault};

/// Moving one note into the tag's directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Move {
    pub from: NotePath,
    pub to: NotePath,
}

/// What organising a vault will do, or did.
#[derive(Debug, Default)]
pub struct OrganisePlan {
    pub moves: Vec<Move>,
    /// Notes left in place because the directory already has a note with
    /// the same name.
    pub clashes: Vec<NotePath>,
    pub errors: Vec<Error>,
}

impl fmt::Display for OrganisePlan {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} moved, {} clashes, {} errors",
            self.moves.len(),
            self.clashes.len(),
            self.errors.len()
        )
    }
}

enum Placement {
    Move(Move),
    Clash(NotePath),
}

/// Works out which notes in the root of `vault` tagged `tag` move into a
/// subdirectory, without moving any.
///
/// The directory is named `dir_name`, or `tag` if not given. Only notes
/// directly in the vault root are considered.
pub fn plan_moves(vault: &Vault, tag: &Tag, dir_name: Option<&str>) -> Result<OrganisePlan> {
    let dir = Path::new(dir_name.unwrap_or(tag.as_str()));

    let mut names: Vec<_> = fs::read_dir(vault.root())
        .ctx("read directory", vault.root())?
        .map(|entry| entry.map(|e| e.path()))
        .collect::<std::io::Result<Vec<_>>>()
        .ctx("read directory", vault.root())?
        .into_iter()
        .filter(|path| path.is_file() && path.extension().is_some_and(|ext| ext == "md"))
        .filter_map(|path| path.file_name().map(OsStr::to_owned))
        .collect();
    names.sort();

    let Report { done, errors } = names
        .par_iter()
        .map(|name| place(vault, tag, dir, name))
        .filter_map(Result::transpose)
        .collect();

    let mut plan = OrganisePlan {
        errors,
        ..OrganisePlan::default()
    };
    for placement in done {
        match placement {
            Placement::Move(m) => plan.moves.push(m),
            Placement::Clash(path) => plan.clashes.push(path),
        }
    }
    Ok(plan)
}

/// Where a root note goes, or `None` if it doesn't have the tag.
fn place(vault: &Vault, tag: &Tag, dir: &Path, name: &OsStr) -> Result<Option<Placement>> {
    let from = NotePath::new(name);
    if !load_tags(&vault.resolve(&from))?.contains(tag) {
        return Ok(None);
    }
    let to = NotePath::new(dir.join(name));
    Ok(Some(if vault.resolve(&to).exists() {
        Placement::Clash(from)
    } else {
        Placement::Move(Move { from, to })
    }))
}

/// Carries out the moves in `plan`.
///
/// Moves that fail go to `errors`, and any whose target has appeared since
/// planning go to `clashes` rather than overwriting it.
pub fn apply_moves(vault: &Vault, plan: OrganisePlan) -> OrganisePlan {
    let OrganisePlan {
        moves,
        mut clashes,
        mut errors,
    } = plan;
    let mut done = Vec::new();

    for m in moves {
        let (from, to) = (vault.resolve(&m.from), vault.resolve(&m.to));
        if to.exists() {
            clashes.push(m.from);
            continue;
        }
        match move_file(&from, &to) {
            Ok(()) => {
                tracing::info!("moved: {} -> {}", m.from, m.to);
                done.push(m);
            }
            Err(e) => errors.push(e),
        }
    }
    OrganisePlan {
        moves: done,
        clashes,
        errors,
    }
}

fn move_file(from: &Path, to: &Path) -> Result<()> {
    if let Some(dir) = to.parent() {
        fs::create_dir_all(dir).ctx("create directory", dir)?;
    }
    fs::rename(from, to).ctx("move", from)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn game() -> Tag {
        Tag::parse("#Game").unwrap()
    }

    #[test]
    fn moves_tagged_root_notes_without_overwriting() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("games")).unwrap();
        fs::create_dir_all(root.join("sub")).unwrap();
        fs::write(root.join("a.md"), "#game").unwrap();
        fs::write(root.join("b.md"), "---\ntags: [Game]\n---\n").unwrap();
        fs::write(root.join("c.md"), "#game").unwrap();
        fs::write(root.join("games/c.md"), "existing").unwrap();
        fs::write(root.join("d.md"), "untagged").unwrap();
        fs::write(root.join("sub/e.md"), "#game").unwrap();

        let vault = Vault::open(root).unwrap();
        let plan = apply_moves(&vault, plan_moves(&vault, &game(), Some("games")).unwrap());

        let moved: Vec<_> = plan.moves.iter().map(|m| &m.from).collect();
        assert_eq!(moved, [&NotePath::new("a.md"), &NotePath::new("b.md")]);
        assert_eq!(plan.moves[0].to, NotePath::new("games/a.md"));
        assert_eq!(plan.clashes, [NotePath::new("c.md")]);
        assert_eq!(plan.to_string(), "2 moved, 1 clashes, 0 errors");
        assert!(root.join("games/a.md").exists());
        assert_eq!(
            fs::read_to_string(root.join("games/c.md")).unwrap(),
            "existing"
        );
        assert!(root.join("c.md").exists());
        assert!(root.join("d.md").exists());
        assert!(root.join("sub/e.md").exists());
    }

    #[test]
    fn planning_moves_nothing() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.md"), "#game").unwrap();

        let vault = Vault::open(dir.path()).unwrap();
        let plan = plan_moves(&vault, &game(), None).unwrap();

        assert_eq!(plan.moves[0].to, NotePath::new("game/a.md"));
        assert!(dir.path().join("a.md").exists());
        assert!(!dir.path().join("game").exists());
    }

    #[test]
    fn target_appearing_after_planning_is_a_clash() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.md"), "#game").unwrap();
        let vault = Vault::open(dir.path()).unwrap();

        let plan = plan_moves(&vault, &game(), None).unwrap();
        fs::create_dir(dir.path().join("game")).unwrap();
        fs::write(dir.path().join("game/a.md"), "new").unwrap();
        let plan = apply_moves(&vault, plan);

        assert!(plan.moves.is_empty());
        assert_eq!(plan.clashes, [NotePath::new("a.md")]);
        assert_eq!(
            fs::read_to_string(dir.path().join("game/a.md")).unwrap(),
            "new"
        );
    }
}
