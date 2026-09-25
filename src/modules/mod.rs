pub mod targets;

use std::fs;
use std::path::Path;

use anyhow::{Result, bail};
use rayon::prelude::*;

use crate::core::plan::{Item, Plan};
use crate::core::scanner::dir_size;
use targets::{CATEGORIES, Mode, TARGETS, Target};

pub trait Module: Sync {
    fn id(&self) -> &'static str;
    fn description(&self) -> &'static str;
    /// Read-only: builds a plan without touching the disk.
    fn scan(&self, home: &Path) -> Plan;
}

/// A category backed entirely by rows of the target table.
struct TableModule {
    id: &'static str,
    description: &'static str,
}

impl Module for TableModule {
    fn id(&self) -> &'static str {
        self.id
    }

    fn description(&self) -> &'static str {
        self.description
    }

    fn scan(&self, home: &Path) -> Plan {
        let mut plan = Plan::default();
        for target in TARGETS.iter().filter(|t| t.category == self.id) {
            scan_target(home, target, &mut plan);
        }
        plan
    }
}

fn scan_target(home: &Path, target: &Target, plan: &mut Plan) {
    let root = home.join(target.rel_path);
    let paths = match target.mode {
        Mode::Whole => {
            if root.symlink_metadata().is_err() {
                return;
            }
            vec![root]
        }
        Mode::Contents => match fs::read_dir(&root) {
            Ok(rd) => rd
                .filter_map(Result::ok)
                .filter(|e| !target.skip.iter().any(|s| e.file_name() == *s))
                .map(|e| e.path())
                .collect(),
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
                plan.warnings.push(format!(
                    "cannot read {}: grant your terminal Full Disk Access in System Settings > Privacy & Security",
                    root.display()
                ));
                return;
            }
            // Absent directory: nothing to clean.
            Err(_) => return,
        },
    };
    let items: Vec<Item> = paths
        .into_par_iter()
        .map(|path| {
            let size = match path.symlink_metadata() {
                Ok(m) if m.is_dir() => dir_size(&path),
                Ok(m) => m.len(),
                Err(_) => 0,
            };
            Item { path, size, category: target.category.into(), reason: target.reason.into() }
        })
        .collect();
    plan.items.extend(items);
}

pub fn all() -> Vec<Box<dyn Module>> {
    CATEGORIES.iter().map(|&(id, description)| Box::new(TableModule { id, description }) as Box<dyn Module>).collect()
}

/// Scans every module in parallel, in registry order.
pub fn scan_all(home: &Path) -> Vec<(&'static str, &'static str, Plan)> {
    all().par_iter().map(|m| (m.id(), m.description(), m.scan(home))).collect()
}

/// Resolves category names (`all` = everything except trash) and merges their plans.
pub fn build_plan(home: &Path, names: &[String]) -> Result<Plan> {
    let modules = all();
    let valid: Vec<&str> = modules.iter().map(|m| m.id()).collect();
    if names.is_empty() {
        bail!("no category given; choose from: {}, or `all`", valid.join(", "));
    }
    let mut selected: Vec<&str> = Vec::new();
    for name in names {
        if name == "all" {
            selected.extend(valid.iter().filter(|id| **id != "trash"));
        } else if let Some(id) = valid.iter().find(|id| **id == name) {
            selected.push(id);
        } else {
            bail!("unknown category `{name}`; valid categories: {}, all", valid.join(", "));
        }
    }
    selected.sort_unstable();
    selected.dedup();
    let mut merged = Plan::default();
    for m in modules.iter().filter(|m| selected.contains(&m.id())) {
        let plan = m.scan(home);
        merged.items.extend(plan.items);
        merged.warnings.extend(plan.warnings);
    }
    Ok(merged)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::executor::{self, Mode as ExecMode};
    use crate::core::safety::Guard;

    fn write(home: &Path, rel: &str, bytes: usize) {
        let p = home.join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, vec![0u8; bytes]).unwrap();
    }

    fn fixture() -> tempfile::TempDir {
        let d = tempfile::tempdir().unwrap();
        let h = d.path();
        write(h, "Library/Caches/com.foo/data", 100);
        write(h, "Library/Caches/com.apple.Safari/x", 40);
        write(h, "Library/Caches/Homebrew/y", 30);
        write(h, "Library/Logs/app.log", 10);
        write(h, ".Trash/old", 5);
        write(h, ".npm/_cacache/z", 20);
        write(h, "Documents/keep.txt", 999);
        d
    }

    fn names(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn finds_items_and_sizes() {
        let d = fixture();
        let plan = build_plan(d.path(), &names(&["caches"])).unwrap();
        assert_eq!(plan.items.len(), 1);
        assert_eq!(plan.total_size(), 100);
        let dev = build_plan(d.path(), &names(&["dev"])).unwrap();
        assert_eq!(dev.total_size(), 50);
    }

    #[test]
    fn absent_paths_yield_empty_plan() {
        let d = tempfile::tempdir().unwrap();
        for m in all() {
            assert!(m.scan(d.path()).items.is_empty(), "{}", m.id());
        }
    }

    #[test]
    fn never_proposes_protected_paths() {
        let d = fixture();
        let guard = Guard::new(d.path()).unwrap();
        for (_, _, plan) in scan_all(d.path()) {
            for item in plan.items {
                assert!(guard.check(&item.path).is_ok(), "{}", item.path.display());
            }
        }
    }

    #[test]
    fn all_excludes_trash() {
        let d = fixture();
        let plan = build_plan(d.path(), &names(&["all"])).unwrap();
        assert!(plan.items.iter().all(|i| i.category != "trash"));
        assert!(!plan.items.is_empty());
    }

    #[test]
    fn unknown_or_missing_category_errors() {
        let d = fixture();
        let err = build_plan(d.path(), &names(&["bogus"])).unwrap_err().to_string();
        assert!(err.contains("caches") && err.contains("bogus"));
        assert!(build_plan(d.path(), &[]).is_err());
    }

    #[test]
    fn dry_run_leaves_fixture_and_permanent_removes_planned_items() {
        let d = fixture();
        let guard = Guard::new(d.path()).unwrap();
        let plan = build_plan(d.path(), &names(&["caches", "logs"])).unwrap();
        executor::execute(&plan, &guard, ExecMode::DryRun, None);
        assert!(d.path().join("Library/Caches/com.foo").exists());

        let r = executor::execute(&plan, &guard, ExecMode::Permanent, None);
        assert_eq!(r.removed.len(), plan.items.len());
        assert!(!d.path().join("Library/Caches/com.foo").exists());
        assert!(!d.path().join("Library/Logs/app.log").exists());
        // Unclaimed and unrelated data survives.
        assert!(d.path().join("Library/Caches").exists());
        assert!(d.path().join("Library/Caches/com.apple.Safari/x").exists());
        assert!(d.path().join("Documents/keep.txt").exists());
    }
}
