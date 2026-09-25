//! Installed-app discovery and uninstall planning.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Result, bail};
use rayon::prelude::*;

use crate::core::plan::{Category, Item, Plan};
use crate::core::scanner::dir_size;

/// Per-app locations under `~`, matched by exact bundle ID: (directory, file-name suffix).
const LEFTOVER_LOCATIONS: &[(&str, &str)] = &[
    ("Library/Application Support", ""),
    ("Library/Caches", ""),
    ("Library/Preferences", ".plist"),
    ("Library/Saved Application State", ".savedState"),
    ("Library/Containers", ""),
    ("Library/Logs", ""),
    ("Library/HTTPStorages", ""),
    ("Library/WebKit", ""),
    ("Library/Cookies", ".binarycookies"),
    ("Library/Application Scripts", ""),
    ("Library/LaunchAgents", ".plist"),
];

/// Directories where a folder named after the app (not its bundle ID) is only a guess.
const FUZZY_LOCATIONS: &[&str] = &["Library/Application Support", "Library/Caches", "Library/Logs"];

#[derive(Debug, Clone)]
pub struct App {
    pub path: PathBuf,
    /// Display name, from Info.plist or the bundle's file name.
    pub name: String,
    pub bundle_id: Option<String>,
    pub version: Option<String>,
    pub size: u64,
}

impl App {
    pub fn is_apple(&self) -> bool {
        self.bundle_id.as_deref().is_some_and(|id| id.starts_with("com.apple."))
    }

    /// True if any running executable lives inside this bundle.
    pub fn is_running(&self, running: &[PathBuf]) -> bool {
        running.iter().any(|p| p.starts_with(&self.path))
    }

    fn stem(&self) -> String {
        self.path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
    }
}

fn read_app(path: PathBuf) -> App {
    let info = plist::Value::from_file(path.join("Contents/Info.plist")).ok();
    let get = |key: &str| {
        info.as_ref().and_then(|v| v.as_dictionary()).and_then(|d| d.get(key)).and_then(|v| v.as_string()).map(str::to_owned)
    };
    let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let name = get("CFBundleDisplayName").or_else(|| get("CFBundleName")).unwrap_or(stem);
    let size = dir_size(&path);
    App { bundle_id: get("CFBundleIdentifier"), version: get("CFBundleShortVersionString"), name, size, path }
}

/// Lists `*.app` bundles directly inside each directory, sorted by name.
pub fn discover(dirs: &[PathBuf]) -> Vec<App> {
    let bundles: Vec<PathBuf> = dirs
        .iter()
        .filter_map(|d| fs::read_dir(d).ok())
        .flat_map(|rd| rd.filter_map(Result::ok).map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e == "app") && p.is_dir())
        .collect();
    let mut apps: Vec<App> = bundles.into_par_iter().map(read_app).collect();
    apps.sort_by_key(|a| a.name.to_lowercase());
    apps
}

/// Finds one app by bundle ID, display name or file name (case-insensitive).
pub fn find<'a>(apps: &'a [App], query: &str) -> Result<&'a App> {
    let q = query.trim_end_matches(".app").to_lowercase();
    let hits: Vec<&App> = apps
        .iter()
        .filter(|a| {
            a.bundle_id.as_deref().is_some_and(|id| id.to_lowercase() == q)
                || a.name.to_lowercase() == q
                || a.stem().to_lowercase() == q
        })
        .collect();
    match hits.as_slice() {
        [one] => Ok(one),
        [] => bail!("no installed app matches `{query}`; see `klinit apps`"),
        many => bail!(
            "`{query}` is ambiguous: {}",
            many.iter().map(|a| a.path.display().to_string()).collect::<Vec<_>>().join(", ")
        ),
    }
}

pub struct UninstallPlan {
    /// The bundle plus leftovers matched by exact bundle ID.
    pub plan: Plan,
    /// Folders that only match the app's name. Never deleted unless the caller opts in.
    pub fuzzy: Vec<Item>,
}

/// Read-only. Refuses `com.apple.*` apps.
pub fn uninstall_plan(app: &App, home: &Path) -> Result<UninstallPlan> {
    if app.is_apple() {
        bail!("{} is a system app (com.apple.*) and is never uninstalled", app.name);
    }
    let mut plan = Plan::default();
    plan.items.push(Item::from_path(app.path.clone(), Category::App, "application bundle"));

    if let Some(id) = &app.bundle_id {
        for (dir, suffix) in LEFTOVER_LOCATIONS {
            let exact = home.join(dir).join(format!("{id}{suffix}"));
            if exact.symlink_metadata().is_ok() {
                plan.items.push(Item::from_path(exact, Category::Leftover, format!("matches bundle ID {id}")));
            }
        }
        // Group containers are named `group.<id>` or `<TeamID>.<id>`.
        if let Ok(rd) = fs::read_dir(home.join("Library/Group Containers")) {
            for e in rd.filter_map(Result::ok) {
                let n = e.file_name().to_string_lossy().into_owned();
                if n == *id || n.ends_with(&format!(".{id}")) {
                    plan.items.push(Item::from_path(e.path(), Category::Leftover, format!("group container of {id}")));
                }
            }
        }
    } else {
        plan.warnings.push(format!("{} has no bundle ID; leftovers cannot be matched exactly", app.name));
    }

    let mut fuzzy = Vec::new();
    for dir in FUZZY_LOCATIONS {
        for name in [app.name.as_str(), &app.stem()] {
            let p = home.join(dir).join(name);
            if p.symlink_metadata().is_ok() && !fuzzy.iter().any(|i: &Item| i.path == p) {
                fuzzy.push(Item::from_path(p, Category::Fuzzy, format!("folder named after {}", app.name)));
            }
        }
    }
    Ok(UninstallPlan { plan, fuzzy })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_app(dir: &Path, stem: &str, id: &str) -> PathBuf {
        let app = dir.join(format!("{stem}.app"));
        fs::create_dir_all(app.join("Contents/MacOS")).unwrap();
        fs::write(app.join("Contents/MacOS/bin"), vec![0u8; 50]).unwrap();
        let mut d = plist::Dictionary::new();
        d.insert("CFBundleIdentifier".into(), id.into());
        d.insert("CFBundleShortVersionString".into(), "1.2".into());
        plist::Value::Dictionary(d).to_file_xml(app.join("Contents/Info.plist")).unwrap();
        app
    }

    fn write(home: &Path, rel: &str) {
        let p = home.join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, vec![0u8; 10]).unwrap();
    }

    fn setup() -> (tempfile::TempDir, tempfile::TempDir, Vec<App>) {
        let home = tempfile::tempdir().unwrap();
        let apps_dir = tempfile::tempdir().unwrap();
        make_app(apps_dir.path(), "Foo", "com.acme.foo");
        make_app(apps_dir.path(), "Safari", "com.apple.Safari");
        let apps = discover(&[apps_dir.path().to_path_buf()]);
        (home, apps_dir, apps)
    }

    #[test]
    fn discovers_bundle_metadata() {
        let (_h, _a, apps) = setup();
        let foo = find(&apps, "foo").unwrap();
        assert_eq!(foo.bundle_id.as_deref(), Some("com.acme.foo"));
        assert_eq!(foo.version.as_deref(), Some("1.2"));
        assert!(foo.size >= 50);
        assert!(find(&apps, "com.acme.foo").is_ok());
        assert!(find(&apps, "nope").is_err());
    }

    #[test]
    fn exact_leftovers_by_bundle_id_and_fuzzy_separate() {
        let (home, _a, apps) = setup();
        let h = home.path();
        write(h, "Library/Application Support/com.acme.foo/data");
        write(h, "Library/Preferences/com.acme.foo.plist");
        write(h, "Library/Group Containers/ABCDE.com.acme.foo/x");
        write(h, "Library/Caches/com.acme.foobar/unrelated");
        write(h, "Library/Application Support/Foo/data");
        let up = uninstall_plan(find(&apps, "Foo").unwrap(), h).unwrap();
        let paths: Vec<_> = up.plan.items.iter().map(|i| i.path.clone()).collect();
        assert_eq!(paths.len(), 4); // bundle + 3 exact
        assert!(!paths.iter().any(|p| p.ends_with("com.acme.foobar")));
        assert_eq!(up.fuzzy.len(), 1);
        assert!(up.fuzzy[0].path.ends_with("Application Support/Foo"));
    }

    #[test]
    fn apple_apps_are_refused() {
        let (home, _a, apps) = setup();
        assert!(uninstall_plan(find(&apps, "Safari").unwrap(), home.path()).is_err());
    }

    #[test]
    fn running_check_uses_bundle_path() {
        let (_h, _a, apps) = setup();
        let foo = find(&apps, "Foo").unwrap();
        assert!(foo.is_running(&[foo.path.join("Contents/MacOS/bin")]));
        assert!(!foo.is_running(&[PathBuf::from("/usr/bin/ssh")]));
    }
}
