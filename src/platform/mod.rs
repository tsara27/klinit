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

/// (used, total) bytes of the volume holding `path`, from `df`. Used = total - available,
/// which on APFS counts the whole shared container rather than just one volume.
pub fn disk_usage(path: &std::path::Path) -> Option<(u64, u64)> {
    let out = Command::new("df").arg("-k").arg(path).output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let fields: Vec<&str> = text.lines().nth(1)?.split_whitespace().collect();
    let total: u64 = fields.get(1)?.parse().ok()?;
    let avail: u64 = fields.get(3)?.parse().ok()?;
    Some(((total - avail.min(total)) * 1024, total * 1024))
}
