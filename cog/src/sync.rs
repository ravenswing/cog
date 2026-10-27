//! One-way sync from a source vault into a destination vault.
//!
//! Syncing happens in two steps. [`plan_sync`] only reads both vaults and
//! decides what should happen to each note, which is all a dry run needs.
//! [`apply`] then carries out the plan.
//!
//! Whether a note changed is judged by comparing its mtime with the time of
//! the last sync. Copies keep the source's mtime, so a note that was just
//! copied is seen as unchanged on the next run. If that run's sync time
//! wasn't recorded, both copies look changed, so a note whose copies have
//! the same mtime and contents is treated as unchanged too.

use rayon::prelude::*;
use std::fmt;
use std::fs;
use std::io::ErrorKind;
use std::path::Path;
use std::time::SystemTime;

use crate::config::SyncConfig;
use crate::error::{Error, IoContext, Result};
use crate::fsutil::copy_preserving_mtime;
use crate::note::load_tags;
use crate::report::Report;
use crate::tags::Tag;
use crate::vault::{NotePath, Vault};

/// How a note in both vaults has changed since the last sync.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change {
    Neither,
    Source,
    Destination,
    Both,
}

/// Compares both copies of a note against the last sync time.
///
/// A note counts as changed only if its mtime is strictly after the last
/// sync.
pub fn change(src: SystemTime, dest: SystemTime, last_sync: SystemTime) -> Change {
    match (src > last_sync, dest > last_sync) {
        (false, false) => Change::Neither,
        (true, false) => Change::Source,
        (false, true) => Change::Destination,
        (true, true) => Change::Both,
    }
}

/// What sync will do with one note.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Copy a note that only exists in the source.
    Create,
    /// Copy the source over the destination.
    Update,
    /// Copy the destination back over the source.
    BackUpdate,
    /// Both copies changed, so neither is touched.
    Conflict,
    Skip(SkipReason),
}

impl fmt::Display for Action {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Create => write!(f, "create"),
            Self::Update => write!(f, "update"),
            Self::BackUpdate => write!(f, "back-update"),
            Self::Conflict => write!(f, "conflict"),
            Self::Skip(reason) => write!(f, "skip ({reason})"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkipReason {
    Unchanged,
    ExcludedDir,
    ExcludedTag(Tag),
    /// Only the destination changed and back-updating is off.
    DestinationNewer,
}

impl fmt::Display for SkipReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unchanged => write!(f, "unchanged"),
            Self::ExcludedDir => write!(f, "in an excluded directory"),
            Self::ExcludedTag(tag) => write!(f, "tagged #{tag}"),
            Self::DestinationNewer => write!(f, "only changed in the destination"),
        }
    }
}

/// A note's path, the same in both vaults, and what to do with it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Planned {
    pub path: NotePath,
    pub action: Action,
}

/// Everything a sync will do, worked out without writing anything.
#[derive(Debug)]
pub struct SyncPlan {
    /// When planning started, recorded as the new last sync time.
    pub started: SystemTime,
    /// One entry per note, sorted by path.
    pub notes: Vec<Planned>,
    /// Notes that could not be read, and so are left alone.
    pub errors: Vec<Error>,
}

impl SyncPlan {
    /// The time to record as the last sync, or `None` if any note failed.
    ///
    /// Recording a sync with failures would make the failed notes look
    /// unchanged on the next run, so they would never be retried.
    pub fn completed_at(&self) -> Option<SystemTime> {
        self.errors.is_empty().then_some(self.started)
    }

    /// How many notes have each action, and how many failed.
    #[expect(
        clippy::arithmetic_side_effects,
        reason = "each count is at most the number of notes"
    )]
    pub fn summary(&self) -> Summary {
        let summary = Summary {
            errors: self.errors.len(),
            ..Summary::default()
        };
        self.notes.iter().fold(summary, |mut summary, note| {
            match note.action {
                Action::Create => summary.created += 1,
                Action::Update => summary.updated += 1,
                Action::BackUpdate => summary.back_updated += 1,
                Action::Conflict => summary.conflicts += 1,
                Action::Skip(_) => summary.skipped += 1,
            }
            summary
        })
    }
}

/// Counts of what a sync did, or would do.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Summary {
    pub created: usize,
    pub updated: usize,
    pub back_updated: usize,
    pub conflicts: usize,
    pub skipped: usize,
    pub errors: usize,
}

impl fmt::Display for Summary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} created, {} updated, {} back-updated, {} conflicts, {} skipped, {} errors",
            self.created,
            self.updated,
            self.back_updated,
            self.conflicts,
            self.skipped,
            self.errors
        )
    }
}

/// Works out what syncing `src` into `dest` would do.
///
/// Fails if the vaults overlap, or the source vault can't be walked. Notes
/// that can't be read are collected in [`SyncPlan::errors`] and left out of
/// the plan.
pub fn plan_sync(
    src: &Vault,
    dest: &Vault,
    last_sync: SystemTime,
    config: &SyncConfig,
    back_update: bool,
) -> Result<SyncPlan> {
    check_separate(src, dest)?;
    let started = SystemTime::now();
    let src_paths: Vec<NotePath> = src.notes(&config.skip_dirs).collect::<Result<_>>()?;

    let planner = Planner {
        src,
        dest,
        last_sync,
        config,
        back_update,
    };

    let Report {
        done: notes,
        errors,
    } = src_paths
        .into_par_iter()
        .map(|path| {
            let action = planner.plan(&path)?;
            Ok(Planned { path, action })
        })
        .collect();
    Ok(SyncPlan {
        started,
        notes,
        errors,
    })
}

struct Planner<'a> {
    src: &'a Vault,
    dest: &'a Vault,
    last_sync: SystemTime,
    config: &'a SyncConfig,
    back_update: bool,
}

impl Planner<'_> {
    fn plan(&self, path: &NotePath) -> Result<Action> {
        if self.in_excluded_dir(path) {
            return Ok(Action::Skip(SkipReason::ExcludedDir));
        }

        // Check mtimes first, so unchanged notes are never read
        let (src_path, dest_path) = (self.src.resolve(path), self.dest.resolve(path));
        let action = match dest_mtime(&dest_path)? {
            Some(dest) => {
                let src = mtime(&src_path)?;
                match change(src, dest, self.last_sync) {
                    Change::Neither => return Ok(Action::Skip(SkipReason::Unchanged)),
                    Change::Source => Action::Update,
                    Change::Destination if self.back_update => Action::BackUpdate,
                    Change::Destination => Action::Skip(SkipReason::DestinationNewer),
                    // A copy from a sync whose time wasn't recorded. The
                    // mtime clock is coarse, so two separate edits can share
                    // an mtime too, and only the contents tell them apart
                    Change::Both if src == dest && same_contents(&src_path, &dest_path)? => {
                        return Ok(Action::Skip(SkipReason::Unchanged));
                    }
                    Change::Both => Action::Conflict,
                }
            }
            None => Action::Create,
        };

        match self.excluded_tag(path)? {
            Some(tag) => Ok(Action::Skip(SkipReason::ExcludedTag(tag))),
            None => Ok(action),
        }
    }

    fn in_excluded_dir(&self, path: &NotePath) -> bool {
        path.as_path()
            .parent()
            .into_iter()
            .flat_map(Path::components)
            .filter_map(|c| c.as_os_str().to_str())
            .any(|dir| self.config.exclude_dirs.contains(dir))
    }

    fn excluded_tag(&self, path: &NotePath) -> Result<Option<Tag>> {
        let tags = load_tags(&self.src.resolve(path))?;
        Ok(tags.intersection(&self.config.exclude_tags).next().cloned())
    }
}

/// Fails if `src` and `dest` are the same directory, or one is inside the
/// other, which would copy notes back into the vault they came from.
///
/// Both roots are resolved first, so a symlink or `..` can't hide an overlap.
fn check_separate(src: &Vault, dest: &Vault) -> Result<()> {
    let resolve = |vault: &Vault| fs::canonicalize(vault.root()).ctx("resolve", vault.root());
    let (src, dest) = (resolve(src)?, resolve(dest)?);
    if src.starts_with(&dest) || dest.starts_with(&src) {
        return Err(Error::OverlappingVaults { src, dest });
    }
    Ok(())
}

fn same_contents(a: &Path, b: &Path) -> Result<bool> {
    Ok(fs::read(a).ctx("read", a)? == fs::read(b).ctx("read", b)?)
}

fn mtime(path: &Path) -> Result<SystemTime> {
    fs::metadata(path)
        .and_then(|m| m.modified())
        .ctx("read mtime of", path)
}

/// The mtime of a note in the destination, or `None` if it isn't there.
///
/// Looking each note up directly is cheaper than walking the destination,
/// which is usually the larger vault.
fn dest_mtime(path: &Path) -> Result<Option<SystemTime>> {
    match fs::metadata(path) {
        Ok(meta) if meta.is_file() => meta.modified().map(Some).ctx("read mtime of", path),
        Ok(_) => Ok(None),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e).ctx("read mtime of", path),
    }
}

/// Carries out the copies in `plan`.
///
/// A failed copy doesn't stop the others. Notes that fail move from `notes`
/// to `errors`, so the returned plan describes what was actually done.
pub fn apply(plan: SyncPlan, src: &Vault, dest: &Vault) -> SyncPlan {
    let SyncPlan {
        started,
        notes,
        mut errors,
    } = plan;
    let applied: Report<Planned> = notes
        .into_par_iter()
        .map(|note| {
            let (from, to) = match note.action {
                Action::Create | Action::Update => (src, dest),
                Action::BackUpdate => (dest, src),
                Action::Conflict | Action::Skip(_) => return Ok(note),
            };
            copy_preserving_mtime(&from.resolve(&note.path), &to.resolve(&note.path))?;
            tracing::info!("{}: {}", note.action, note.path);
            Ok(note)
        })
        .collect();
    errors.extend(applied.errors);
    SyncPlan {
        started,
        notes: applied.done,
        errors,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, UNIX_EPOCH};

    #[test]
    fn rejects_overlapping_vaults() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        for sub in ["a/inner", "b"] {
            fs::create_dir_all(root.join(sub)).unwrap();
        }
        let vault = |path: &str| Vault::open(root.join(path)).unwrap();

        for (src, dest) in [("a", "a/inner"), ("a/inner", "a"), ("a", "a/inner/..")] {
            assert!(
                matches!(
                    check_separate(&vault(src), &vault(dest)),
                    Err(Error::OverlappingVaults { .. })
                ),
                "{src} {dest}"
            );
        }
        check_separate(&vault("a"), &vault("b")).unwrap();
    }

    #[test]
    fn summary_counts_each_action_once() {
        let planned = |name: &str, action| Planned {
            path: NotePath::new(name),
            action,
        };
        let plan = SyncPlan {
            started: UNIX_EPOCH,
            notes: vec![
                planned("a.md", Action::Create),
                planned("b.md", Action::Create),
                planned("c.md", Action::Conflict),
                planned("d.md", Action::Skip(SkipReason::Unchanged)),
                planned("e.md", Action::Skip(SkipReason::ExcludedDir)),
            ],
            errors: Vec::new(),
        };
        assert_eq!(
            plan.summary().to_string(),
            "2 created, 0 updated, 0 back-updated, 1 conflicts, 2 skipped, 0 errors"
        );
    }

    #[test]
    fn change_against_last_sync() {
        let at = |secs| UNIX_EPOCH + Duration::from_secs(secs);
        let last = at(100);
        let cases = [
            (at(50), at(50), Change::Neither),
            (at(100), at(100), Change::Neither),
            (at(150), at(50), Change::Source),
            (at(150), at(100), Change::Source),
            (at(50), at(150), Change::Destination),
            (at(150), at(150), Change::Both),
            (at(150), at(200), Change::Both),
        ];
        for (src, dest, expected) in cases {
            assert_eq!(change(src, dest, last), expected, "{src:?} {dest:?}");
        }
    }
}
