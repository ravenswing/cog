//! One-way sync from a source vault into a destination vault.
//!
//! Syncing happens in two steps. [`plan_sync`] only reads both vaults and
//! decides what should happen to each note, which is all a dry run needs.
//! [`apply`] then carries out the plan.
//!
//! Whether a note changed is judged by comparing its mtime with the time of
//! the last sync. Copies keep the source's mtime, so a note that was just
//! copied is seen as unchanged on the next run.

use rayon::prelude::*;
use std::collections::BTreeSet;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::config::SyncConfig;
use crate::error::{Error, IoContext, Result};
use crate::fsutil::copy_preserving_mtime;
use crate::note::Note;
use crate::vault::Vault;

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
    ExcludedTag(String),
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

/// A note's path, relative to both vault roots, and what to do with it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Planned {
    pub path: PathBuf,
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
    /// Notes with the given action.
    pub fn with<'a>(&'a self, action: &'a Action) -> impl Iterator<Item = &'a Path> {
        self.notes
            .iter()
            .filter(move |n| &n.action == action)
            .map(|n| n.path.as_path())
    }

    pub fn count(&self, action: &Action) -> usize {
        self.with(action).count()
    }

    /// Notes that will be skipped, for any reason.
    pub fn skipped(&self) -> usize {
        self.notes
            .iter()
            .filter(|n| matches!(n.action, Action::Skip(_)))
            .count()
    }
}

/// Works out what syncing `src` into `dest` would do.
///
/// Fails only if either vault can't be walked. Notes that can't be read are
/// collected in [`SyncPlan::errors`] and left out of the plan.
pub fn plan_sync(
    src: &Vault,
    dest: &Vault,
    last_sync: SystemTime,
    config: &SyncConfig,
    back_update: bool,
) -> Result<SyncPlan> {
    let started = SystemTime::now();
    let src_paths = src.note_paths(&config.skip_dirs)?;
    let dest_paths = dest.note_paths(&config.skip_dirs)?;

    let planner = Planner {
        src,
        dest,
        last_sync,
        config,
        exclude_tags: config
            .exclude_tags
            .iter()
            .map(|t| t.to_lowercase())
            .collect(),
        back_update,
    };

    let results: Vec<_> = src_paths
        .into_iter()
        .collect::<Vec<_>>()
        .into_par_iter()
        .map(|path| {
            let action = planner.plan(&path, dest_paths.contains(&path))?;
            Ok(Planned { path, action })
        })
        .collect();

    let (notes, errors) = results.into_iter().fold(
        (Vec::new(), Vec::new()),
        |(mut notes, mut errors), result| {
            match result {
                Ok(planned) => notes.push(planned),
                Err(e) => errors.push(e),
            }
            (notes, errors)
        },
    );
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
    exclude_tags: BTreeSet<String>,
    back_update: bool,
}

impl Planner<'_> {
    fn plan(&self, path: &Path, in_dest: bool) -> Result<Action> {
        if self.in_excluded_dir(path) {
            return Ok(Action::Skip(SkipReason::ExcludedDir));
        }

        // Check mtimes first, so unchanged notes are never read
        let change = if in_dest {
            let change = change(
                mtime(&self.src.root().join(path))?,
                mtime(&self.dest.root().join(path))?,
                self.last_sync,
            );
            if change == Change::Neither {
                return Ok(Action::Skip(SkipReason::Unchanged));
            }
            Some(change)
        } else {
            None
        };

        if let Some(tag) = self.excluded_tag(path)? {
            return Ok(Action::Skip(SkipReason::ExcludedTag(tag)));
        }

        Ok(match change {
            None => Action::Create,
            Some(Change::Source) => Action::Update,
            Some(Change::Destination) if self.back_update => Action::BackUpdate,
            Some(Change::Destination) => Action::Skip(SkipReason::DestinationNewer),
            Some(Change::Both) => Action::Conflict,
            Some(Change::Neither) => unreachable!("returned early above"),
        })
    }

    fn in_excluded_dir(&self, path: &Path) -> bool {
        path.parent().is_some_and(|parent| {
            parent.components().any(|c| {
                self.config
                    .exclude_dirs
                    .iter()
                    .any(|d| c.as_os_str() == d.as_str())
            })
        })
    }

    fn excluded_tag(&self, path: &Path) -> Result<Option<String>> {
        let note = Note::load(&self.src.root().join(path))?;
        Ok(note
            .tags
            .into_iter()
            .find(|tag| self.exclude_tags.contains(tag)))
    }
}

fn mtime(path: &Path) -> Result<SystemTime> {
    fs::metadata(path)
        .and_then(|m| m.modified())
        .ctx("read mtime of", path)
}

/// Carries out the copies in `plan`, returning any that failed.
///
/// A failed copy doesn't stop the others.
pub fn apply(plan: &SyncPlan, src: &Vault, dest: &Vault) -> Vec<Error> {
    plan.notes
        .iter()
        .filter_map(|note| {
            let (from, to) = match note.action {
                Action::Create | Action::Update => (src, dest),
                Action::BackUpdate => (dest, src),
                Action::Conflict | Action::Skip(_) => return None,
            };
            tracing::info!("{}: {}", note.action, note.path.display());
            copy_preserving_mtime(&from.root().join(&note.path), &to.root().join(&note.path)).err()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, UNIX_EPOCH};

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
