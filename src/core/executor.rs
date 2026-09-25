use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use super::plan::Plan;
use super::safety::Guard;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// Report what would happen; touch nothing.
    DryRun,
    Trash,
    Permanent,
}

#[derive(Debug, Default)]
pub struct Report {
    pub removed: Vec<(PathBuf, u64)>,
    pub skipped: Vec<(PathBuf, String)>,
}

impl Report {
    pub fn freed(&self) -> u64 {
        self.removed.iter().map(|(_, s)| s).sum()
    }
}

/// The only place that deletes anything. Every item is re-checked by the guard at
/// execution time, so a stale or tampered plan cannot bypass it. Actions are appended to `log`.
pub fn execute(plan: &Plan, guard: &Guard, mode: Mode, log: Option<&Path>) -> Report {
    let mut report = Report::default();
    for item in &plan.items {
        let real = match guard.check(&item.path) {
            Ok(p) => p,
            Err(v) => {
                report.skipped.push((item.path.clone(), v.to_string()));
                continue;
            }
        };
        let result = match mode {
            Mode::DryRun => Ok(()),
            Mode::Trash => trash::delete(&real).map_err(|e| e.to_string()),
            Mode::Permanent => remove(&real).map_err(|e| e.to_string()),
        };
        match result {
            Ok(()) => {
                if mode != Mode::DryRun {
                    append_log(log, &format!("{mode:?} {} ({} bytes)", real.display(), item.size));
                }
                report.removed.push((real, item.size));
            }
            Err(e) => report.skipped.push((item.path.clone(), e)),
        }
    }
    report
}

fn remove(path: &Path) -> std::io::Result<()> {
    // symlink_metadata so a symlink is unlinked, not followed.
    if fs::symlink_metadata(path)?.is_dir() { fs::remove_dir_all(path) } else { fs::remove_file(path) }
}

fn append_log(log: Option<&Path>, line: &str) {
    let Some(path) = log else { return };
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(f, "{line}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::plan::Item;

    fn item(path: PathBuf) -> Item {
        Item { path, size: 1, category: "test".into(), reason: "test".into() }
    }

    fn fixture() -> (tempfile::TempDir, Guard) {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("Library/Caches/app")).unwrap();
        fs::write(dir.path().join("Library/Caches/app/f"), "x").unwrap();
        fs::create_dir_all(dir.path().join("Documents")).unwrap();
        let g = Guard::new(dir.path()).unwrap();
        (dir, g)
    }

    #[test]
    fn dry_run_touches_nothing() {
        let (dir, g) = fixture();
        let target = dir.path().join("Library/Caches/app");
        let plan = Plan { items: vec![item(target.clone())] };
        let r = execute(&plan, &g, Mode::DryRun, None);
        assert_eq!(r.removed.len(), 1);
        assert!(target.exists());
    }

    #[test]
    fn permanent_removes_and_logs() {
        let (dir, g) = fixture();
        let target = dir.path().join("Library/Caches/app");
        let log = dir.path().join("state/actions.log");
        let plan = Plan { items: vec![item(target.clone())] };
        let r = execute(&plan, &g, Mode::Permanent, Some(&log));
        assert_eq!(r.removed.len(), 1);
        assert!(!target.exists());
        assert!(fs::read_to_string(log).unwrap().contains("Permanent"));
    }

    #[test]
    fn protected_items_are_skipped_even_in_plan() {
        let (dir, g) = fixture();
        let docs = dir.path().join("Documents");
        let plan = Plan { items: vec![item(docs.clone())] };
        let r = execute(&plan, &g, Mode::Permanent, None);
        assert!(r.removed.is_empty());
        assert_eq!(r.skipped.len(), 1);
        assert!(docs.exists());
    }
}
