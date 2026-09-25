//! Orphaned support files: reverse-DNS named entries whose app is no longer installed.
//! Lower confidence than `uninstall`, so it only reports unless the caller opts in.

use std::collections::HashSet;
use std::fs;
use std::path::Path;

use crate::core::plan::{Category, Item, Plan};
use crate::core::scanner::path_size;

const LOCATIONS: &[(&str, &str)] = &[
    ("Library/Application Support", ""),
    ("Library/Caches", ""),
    ("Library/Preferences", ".plist"),
    ("Library/Saved Application State", ".savedState"),
    ("Library/Containers", ""),
    ("Library/HTTPStorages", ""),
    ("Library/WebKit", ""),
];

/// Looks like a bundle ID: at least three dot-separated parts, no spaces.
fn looks_like_bundle_id(s: &str) -> bool {
    !s.contains(' ') && s.split('.').count() >= 3 && s.split('.').all(|p| !p.is_empty())
}

/// `installed` holds bundle IDs of every installed app. An entry belongs to an app if it equals
/// its ID or extends it (helpers such as `com.foo.app.helper`).
pub fn scan(home: &Path, installed: &HashSet<String>) -> Plan {
    let mut plan = Plan::default();
    for (dir, suffix) in LOCATIONS {
        let Ok(rd) = fs::read_dir(home.join(dir)) else { continue };
        for e in rd.filter_map(Result::ok) {
            let file_name = e.file_name().to_string_lossy().into_owned();
            let Some(id) = file_name.strip_suffix(suffix) else { continue };
            if !looks_like_bundle_id(id) || id.starts_with("com.apple.") {
                continue;
            }
            let owned = installed.iter().any(|i| id == i || id.starts_with(&format!("{i}.")));
            if owned {
                continue;
            }
            let path = e.path();
            let Some(size) = path_size(&path) else { continue };
            let reason = format!("no installed app has bundle ID {id}");
            plan.items.push(Item { path, size, category: Category::Orphaned, reason });
        }
    }
    plan.items.sort_by_key(|i| std::cmp::Reverse(i.size));
    plan
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(home: &Path, rel: &str) {
        let p = home.join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, [0u8; 10]).unwrap();
    }

    #[test]
    fn reports_only_orphans() {
        let d = tempfile::tempdir().unwrap();
        let h = d.path();
        write(h, "Library/Application Support/com.gone.app/x");
        write(h, "Library/Preferences/com.gone.app.plist");
        write(h, "Library/Caches/com.here.app/x");
        write(h, "Library/Caches/com.here.app.helper/x");
        write(h, "Library/Caches/com.apple.Safari/x");
        write(h, "Library/Caches/Homebrew/x");
        let installed = HashSet::from(["com.here.app".to_string()]);
        let plan = scan(h, &installed);
        let mut names: Vec<_> = plan.items.iter().map(|i| i.path.file_name().unwrap().to_string_lossy().into_owned()).collect();
        names.sort();
        assert_eq!(names, ["com.gone.app", "com.gone.app.plist"]);
    }
}
