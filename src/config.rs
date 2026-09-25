//! Optional `~/.config/klinit/config.toml`.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// Paths klinit must never touch, in addition to the built-in protected set.
    /// Relative paths are resolved against the home directory.
    #[serde(default)]
    pub exclude: Vec<String>,
    /// Categories `klinit clean` uses when none are given.
    #[serde(default)]
    pub default_categories: Vec<String>,
}

pub fn path(home: &Path) -> PathBuf {
    home.join(".config/klinit/config.toml")
}

/// A missing file means defaults; a malformed one is an error so exclusions are never silently lost.
pub fn load(home: &Path) -> Result<Config> {
    let p = path(home);
    match fs::read_to_string(&p) {
        Ok(text) => toml::from_str(&text).with_context(|| format!("invalid config {}", p.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Config::default()),
        Err(e) => Err(e).with_context(|| format!("cannot read {}", p.display())),
    }
}

impl Config {
    pub fn exclude_paths(&self, home: &Path) -> Vec<PathBuf> {
        self.exclude.iter().map(|e| home.join(e)).collect() // join keeps absolute paths as-is
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_is_default_and_bad_is_error() {
        let d = tempfile::tempdir().unwrap();
        assert_eq!(load(d.path()).unwrap(), Config::default());
        let p = path(d.path());
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(&p, "exclude = [\"Projects\"]\ndefault_categories = [\"caches\"]\n").unwrap();
        let c = load(d.path()).unwrap();
        assert_eq!(c.exclude_paths(d.path()), vec![d.path().join("Projects")]);
        fs::write(&p, "bogus = 1").unwrap();
        assert!(load(d.path()).is_err());
    }
}
