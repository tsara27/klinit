use std::path::Path;

use rayon::prelude::*;
use walkdir::WalkDir;

/// Total size in bytes of `path`. Symlinks are counted as links, never followed.
/// Unreadable entries are skipped.
pub fn dir_size(path: &Path) -> u64 {
    WalkDir::new(path)
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
        .filter_map(|e| e.metadata().ok())
        .filter(|m| m.is_file())
        .map(|m| m.len())
        .sum()
}

/// Size of a file or, recursively, a directory. Symlinks are not followed. `None` if unreadable.
pub fn path_size(path: &Path) -> Option<u64> {
    let m = path.symlink_metadata().ok()?;
    Some(if m.is_dir() { dir_size(path) } else { m.len() })
}

/// Sizes of the immediate children of `path`, largest first, computed in parallel.
pub fn child_sizes(path: &Path) -> std::io::Result<Vec<(std::path::PathBuf, u64)>> {
    let entries: Vec<_> = std::fs::read_dir(path)?.filter_map(Result::ok).map(|e| e.path()).collect();
    let mut sizes: Vec<_> = entries
        .into_par_iter()
        .map(|p| {
            let size = match p.symlink_metadata() {
                Ok(m) if m.is_file() => m.len(),
                Ok(m) if m.is_dir() => dir_size(&p),
                _ => 0,
            };
            (p, size)
        })
        .collect();
    sizes.sort_by_key(|s| std::cmp::Reverse(s.1));
    Ok(sizes)
}

pub fn format_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut v = bytes as f64;
    let mut i = 0;
    while v >= 1000.0 && i < UNITS.len() - 1 {
        v /= 1000.0;
        i += 1;
    }
    if i == 0 { format!("{bytes} B") } else { format!("{v:.1} {}", UNITS[i]) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn sums_nested_files_and_ignores_symlink_targets() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("a")).unwrap();
        fs::write(dir.path().join("a/f1"), vec![0u8; 100]).unwrap();
        fs::write(dir.path().join("f2"), vec![0u8; 50]).unwrap();
        std::os::unix::fs::symlink(dir.path().join("a"), dir.path().join("link")).unwrap();
        assert_eq!(dir_size(dir.path()), 150);
    }

    #[test]
    fn children_sorted_largest_first() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("small"), vec![0u8; 1]).unwrap();
        fs::write(dir.path().join("big"), vec![0u8; 10]).unwrap();
        let sizes = child_sizes(dir.path()).unwrap();
        assert_eq!(sizes[0].1, 10);
    }

    #[test]
    fn formats() {
        assert_eq!(format_size(999), "999 B");
        assert_eq!(format_size(1_500_000), "1.5 MB");
    }
}
