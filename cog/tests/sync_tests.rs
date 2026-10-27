#![expect(
    clippy::unwrap_used,
    clippy::panic,
    reason = "setup failures in shared helpers should fail the test"
)]

use filetime::FileTime;
use std::fs;
use std::path::Path;
use std::time::{Duration, SystemTime};
use tempfile::TempDir;

use cog::config::SyncConfig;
use cog::sync::{Action, SkipReason, SyncPlan, apply, plan_sync};
use cog::vault::Vault;

/// A source and destination vault, with a last sync an hour ago.
struct Fixture {
    _dir: TempDir,
    src: Vault,
    dest: Vault,
    last_sync: SystemTime,
}

impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("src")).unwrap();
        fs::create_dir(dir.path().join("dest")).unwrap();
        Self {
            src: Vault::open(dir.path().join("src")).unwrap(),
            dest: Vault::open(dir.path().join("dest")).unwrap(),
            _dir: dir,
            last_sync: ago(60),
        }
    }

    /// Writes a note into `vault` with an mtime `mins` minutes ago.
    fn write(&self, vault: &Vault, path: &str, content: &str, mins: u64) {
        let path = vault.root().join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, content).unwrap();
        filetime::set_file_mtime(&path, FileTime::from_system_time(ago(mins))).unwrap();
    }

    fn plan(&self, back_update: bool) -> SyncPlan {
        plan_sync(
            &self.src,
            &self.dest,
            self.last_sync,
            &SyncConfig::default(),
            back_update,
        )
        .unwrap()
    }

    /// Plans and applies a sync, then records it as the last sync.
    fn sync(&mut self) -> SyncPlan {
        let plan = self.plan(true);
        let planning_errors = plan.errors.len();
        let plan = self.apply(plan);
        assert_eq!(plan.errors.len(), planning_errors, "a copy failed");
        plan
    }

    /// Applies `plan`, recording the sync time as `main` does.
    fn apply(&mut self, plan: SyncPlan) -> SyncPlan {
        let plan = apply(plan, &self.src, &self.dest);
        if let Some(time) = plan.completed_at() {
            self.last_sync = time;
        }
        plan
    }

    fn action(plan: &SyncPlan, path: &str) -> Action {
        plan.notes
            .iter()
            .find(|n| n.path.as_path() == Path::new(path))
            .map(|n| n.action.clone())
            .unwrap_or_else(|| panic!("{path} not in plan"))
    }
}

fn ago(mins: u64) -> SystemTime {
    SystemTime::now() - Duration::from_secs(mins * 60)
}

fn read(vault: &Vault, path: &str) -> String {
    fs::read_to_string(vault.root().join(path)).unwrap()
}

fn mtime(vault: &Vault, path: &str) -> SystemTime {
    fs::metadata(vault.root().join(path))
        .unwrap()
        .modified()
        .unwrap()
}

#[test]
fn creates_new_notes_and_second_run_does_nothing() {
    let mut f = Fixture::new();
    f.write(&f.src, "a.md", "new", 120);
    f.write(&f.src, "sub/b.md", "nested", 10);

    let plan = f.sync();
    assert_eq!(Fixture::action(&plan, "a.md"), Action::Create);
    assert_eq!(read(&f.dest, "sub/b.md"), "nested");
    assert_eq!(mtime(&f.dest, "a.md"), mtime(&f.src, "a.md"));

    let plan = f.sync();
    assert_eq!(plan.summary().skipped, plan.notes.len());
    assert!(plan.errors.is_empty());
}

#[test]
fn updates_changed_source() {
    let mut f = Fixture::new();
    f.write(&f.src, "a.md", "new", 10);
    f.write(&f.dest, "a.md", "old", 120);

    let plan = f.sync();
    assert_eq!(Fixture::action(&plan, "a.md"), Action::Update);
    assert_eq!(read(&f.dest, "a.md"), "new");
}

#[test]
fn back_updates_changed_destination_only_when_enabled() {
    let mut f = Fixture::new();
    f.write(&f.src, "a.md", "old", 120);
    f.write(&f.dest, "a.md", "new", 10);

    let plan = f.plan(false);
    assert_eq!(
        Fixture::action(&plan, "a.md"),
        Action::Skip(SkipReason::DestinationNewer)
    );

    let plan = f.sync();
    assert_eq!(Fixture::action(&plan, "a.md"), Action::BackUpdate);
    assert_eq!(read(&f.src, "a.md"), "new");
}

#[test]
fn leaves_conflicts_alone() {
    let mut f = Fixture::new();
    f.write(&f.src, "a.md", "src", 10);
    f.write(&f.dest, "a.md", "dest", 5);

    let plan = f.sync();
    assert_eq!(Fixture::action(&plan, "a.md"), Action::Conflict);
    assert_eq!(read(&f.src, "a.md"), "src");
    assert_eq!(read(&f.dest, "a.md"), "dest");
}

#[test]
fn skips_unchanged_notes() {
    let f = Fixture::new();
    f.write(&f.src, "a.md", "same", 120);
    f.write(&f.dest, "a.md", "same", 120);

    let plan = f.plan(true);
    assert_eq!(
        Fixture::action(&plan, "a.md"),
        Action::Skip(SkipReason::Unchanged)
    );
}

#[test]
fn skips_excluded_tags() {
    let mut f = Fixture::new();
    f.write(&f.src, "front.md", "---\ntags: [client]\n---\n", 10);
    f.write(&f.src, "inline.md", "text #no-sync\n", 10);
    f.write(&f.src, "upper.md", "#NBD\n", 10);

    let plan = f.sync();
    for path in ["front.md", "inline.md", "upper.md"] {
        assert!(matches!(
            Fixture::action(&plan, path),
            Action::Skip(SkipReason::ExcludedTag(_))
        ));
        assert!(!f.dest.root().join(path).exists());
    }
}

#[test]
fn skips_excluded_and_skipped_dirs_at_any_depth() {
    let mut f = Fixture::new();
    f.write(&f.src, "meetings/2024/a.md", "", 10);
    f.write(&f.src, "templates/t.md", "", 10);
    f.write(&f.src, "work/templates/t.md", "", 10);

    let plan = f.sync();
    assert_eq!(
        Fixture::action(&plan, "meetings/2024/a.md"),
        Action::Skip(SkipReason::ExcludedDir)
    );
    assert_eq!(plan.notes.len(), 1);
    assert!(fs::read_dir(f.dest.root()).unwrap().next().is_none());
}

#[test]
fn bad_notes_are_reported_and_others_still_sync() {
    let mut f = Fixture::new();
    f.write(&f.src, "bad.md", "---\ntags: [\n---\n", 10);
    f.write(&f.src, "good.md", "fine", 10);

    let plan = f.sync();
    assert_eq!(plan.errors.len(), 1);
    assert!(!f.dest.root().join("bad.md").exists());
    assert_eq!(read(&f.dest, "good.md"), "fine");
}

#[cfg(unix)]
#[test]
fn failed_copies_are_retried_next_run() {
    use std::os::unix::fs::PermissionsExt;
    let set_mode = |vault: &Vault, mode| {
        fs::set_permissions(vault.root(), fs::Permissions::from_mode(mode)).unwrap();
    };
    let mut f = Fixture::new();
    f.write(&f.src, "a.md", "new", 10);
    f.write(&f.dest, "a.md", "old", 120);

    set_mode(&f.dest, 0o555);
    let plan = f.plan(true);
    let plan = f.apply(plan);
    set_mode(&f.dest, 0o755);
    assert_eq!(plan.errors.len(), 1);

    let plan = f.sync();
    assert_eq!(Fixture::action(&plan, "a.md"), Action::Update);
    assert_eq!(read(&f.dest, "a.md"), "new");
}

#[cfg(unix)]
#[test]
fn notes_copied_in_a_failed_run_are_not_conflicts() {
    use std::os::unix::fs::PermissionsExt;
    let mut f = Fixture::new();
    f.write(&f.src, "a.md", "new", 10);
    f.write(&f.src, "sub/b.md", "new", 10);
    f.write(&f.dest, "sub/b.md", "old", 120);
    let sub = f.dest.root().join("sub");

    fs::set_permissions(&sub, fs::Permissions::from_mode(0o555)).unwrap();
    let plan = f.plan(true);
    let plan = f.apply(plan);
    fs::set_permissions(&sub, fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(plan.errors.len(), 1);

    let plan = f.sync();
    assert_eq!(
        Fixture::action(&plan, "a.md"),
        Action::Skip(SkipReason::Unchanged)
    );
    assert_eq!(Fixture::action(&plan, "sub/b.md"), Action::Update);
}

#[test]
fn planning_writes_nothing() {
    let f = Fixture::new();
    f.write(&f.src, "a.md", "new", 10);
    f.write(&f.src, "b.md", "changed", 10);
    f.write(&f.dest, "b.md", "old", 120);

    f.plan(true);
    assert!(!f.dest.root().join("a.md").exists());
    assert_eq!(read(&f.dest, "b.md"), "old");
}
