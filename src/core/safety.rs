use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// Top-level directories under `$HOME` that are never touched, nor is anything inside them.
const PROTECTED_HOME_DIRS: &[&str] = &[
    "Documents", "Desktop", "Pictures", "Movies", "Music", "Downloads", ".ssh", ".gnupg",
    "Library/Keychains",
];

#[derive(Debug, PartialEq, Eq)]
pub enum Violation {
    Missing,
    OutsideHome,
    Protected(PathBuf),
    IsHome,
}

impl std::fmt::Display for Violation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Violation::Missing => write!(f, "path does not exist"),
            Violation::OutsideHome => write!(f, "path is outside the home directory"),
            Violation::Protected(p) => write!(f, "path is inside protected location {}", p.display()),
            Violation::IsHome => write!(f, "refusing to touch the home directory itself"),
        }
    }
}

/// Every path must pass through the guard before the executor may delete it.
pub struct Guard {
    home: PathBuf,
    protected: Vec<PathBuf>,
    /// Directories (e.g. /Applications) whose direct `*.app` children may be deleted despite being outside home.
    app_dirs: Vec<PathBuf>,
}

impl Guard {
    /// `home` is canonicalized so symlinked homes compare correctly.
    pub fn new(home: &Path) -> Result<Self> {
        let home = home.canonicalize().with_context(|| format!("home {} not found", home.display()))?;
        anyhow::ensure!(home.parent().is_some(), "refusing to use / as the home directory");
        // System locations (/System, /usr, ...) need no entry: anything outside home is rejected.
        let protected = PROTECTED_HOME_DIRS.iter().map(|d| home.join(d)).collect();
        Ok(Self { home, protected, app_dirs: Vec::new() })
    }

    /// Allows deleting `*.app` bundles that sit directly inside `dir`. Nothing else there is allowed.
    pub fn allow_app_dir(mut self, dir: &Path) -> Self {
        if let Ok(d) = dir.canonicalize() {
            self.app_dirs.push(d);
        }
        self
    }

    /// Adds user-configured paths that must never be deleted (or anything inside them).
    pub fn protect(mut self, paths: impl IntoIterator<Item = PathBuf>) -> Self {
        // Canonicalize so symlinked spellings match; a missing path is kept as written.
        self.protected.extend(paths.into_iter().map(|p| p.canonicalize().unwrap_or(p)));
        self
    }

    pub fn home(&self) -> &Path {
        &self.home
    }

    /// Canonicalizes `path` (resolving symlinks) and returns it only if deletion is allowed.
    /// A symlink is judged by where it points, so a link out of a cache dir cannot smuggle
    /// a protected target through.
    pub fn check(&self, path: &Path) -> Result<PathBuf, Violation> {
        let real = path.canonicalize().map_err(|_| Violation::Missing)?;
        if real == self.home {
            return Err(Violation::IsHome);
        }
        let is_app_bundle = real.extension().is_some_and(|e| e == "app")
            && real.parent().is_some_and(|p| self.app_dirs.iter().any(|d| d == p));
        if is_app_bundle {
            return Ok(real);
        }
        if !real.starts_with(&self.home) {
            return Err(Violation::OutsideHome);
        }
        for p in &self.protected {
            if real.starts_with(p) {
                return Err(Violation::Protected(p.clone()));
            }
        }
        Ok(real)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn setup() -> (tempfile::TempDir, Guard) {
        let dir = tempfile::tempdir().unwrap();
        for d in ["Documents", "Library/Caches/app", ".ssh"] {
            fs::create_dir_all(dir.path().join(d)).unwrap();
        }
        let guard = Guard::new(dir.path()).unwrap();
        (dir, guard)
    }

    #[test]
    fn allows_cache_dir() {
        let (dir, g) = setup();
        assert!(g.check(&dir.path().join("Library/Caches/app")).is_ok());
    }

    #[test]
    fn rejects_home_itself() {
        let (dir, g) = setup();
        assert_eq!(g.check(dir.path()), Err(Violation::IsHome));
    }

    #[test]
    fn rejects_protected_dirs() {
        let (dir, g) = setup();
        assert!(matches!(g.check(&dir.path().join("Documents")), Err(Violation::Protected(_))));
        assert!(matches!(g.check(&dir.path().join(".ssh")), Err(Violation::Protected(_))));
    }

    #[test]
    fn rejects_outside_home() {
        let (_dir, g) = setup();
        assert_eq!(g.check(Path::new("/tmp")), Err(Violation::OutsideHome));
        assert_eq!(g.check(Path::new("/")), Err(Violation::OutsideHome));
    }

    #[test]
    fn app_dir_allows_only_direct_app_bundles() {
        let (dir, _) = setup();
        let apps = tempfile::tempdir().unwrap();
        fs::create_dir_all(apps.path().join("Foo.app/Contents")).unwrap();
        fs::create_dir_all(apps.path().join("Other")).unwrap();
        let g = Guard::new(dir.path()).unwrap().allow_app_dir(apps.path());
        assert!(g.check(&apps.path().join("Foo.app")).is_ok());
        assert_eq!(g.check(&apps.path().join("Foo.app/Contents")), Err(Violation::OutsideHome));
        assert_eq!(g.check(&apps.path().join("Other")), Err(Violation::OutsideHome));
    }

    #[test]
    fn configured_exclusions_are_protected() {
        let (dir, _) = setup();
        fs::create_dir_all(dir.path().join("Projects/x")).unwrap();
        let g = Guard::new(dir.path()).unwrap().protect([dir.path().join("Projects")]);
        assert!(matches!(g.check(&dir.path().join("Projects/x")), Err(Violation::Protected(_))));
    }

    #[test]
    fn rejects_missing() {
        let (dir, g) = setup();
        assert_eq!(g.check(&dir.path().join("nope")), Err(Violation::Missing));
    }

    #[test]
    fn symlink_to_protected_is_rejected() {
        let (dir, g) = setup();
        let link = dir.path().join("Library/Caches/app/link");
        std::os::unix::fs::symlink(dir.path().join("Documents"), &link).unwrap();
        assert!(matches!(g.check(&link), Err(Violation::Protected(_))));
    }

    #[test]
    fn dotdot_escape_is_rejected() {
        let (dir, g) = setup();
        let sneaky = dir.path().join("Library/Caches/../../Documents");
        assert!(matches!(g.check(&sneaky), Err(Violation::Protected(_))));
    }
}
