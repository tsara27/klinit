//! macOS-specific queries.

use std::path::PathBuf;
use std::process::Command;

/// Executable paths of all running processes (`ps` reports the full path on macOS).
pub fn running_executables() -> Vec<PathBuf> {
    Command::new("ps")
        .args(["-axo", "comm="])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).lines().map(|l| PathBuf::from(l.trim())).collect())
        .unwrap_or_default()
}
