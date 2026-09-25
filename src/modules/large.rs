//! Large/old file finder and duplicate finder. Both are read-only; removal is always opt-in.

use std::collections::HashMap;
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use rayon::prelude::*;
use serde::Serialize;
use walkdir::WalkDir;

use crate::core::plan::{Category, Item, Plan};

#[derive(Debug, Clone, Serialize)]
pub struct FileInfo {
    pub path: PathBuf,
    pub size: u64,
    /// Days since last modification.
    pub age_days: u64,
}

/// Regular files under `root`. Symlinks are not followed; `Library`, the Trash and
/// app/photo-library packages are skipped because their contents are not user files.
fn walk_files(root: &Path) -> Vec<(PathBuf, fs::Metadata)> {
    WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| {
            let name = e.file_name().to_string_lossy();
            let top_level = e.depth() == 1;
            !(top_level && (name == "Library" || name == ".Trash"))
                && !(e.file_type().is_dir()
                    && (name.ends_with(".app") || name.ends_with(".photoslibrary") || name.ends_with(".imovielibrary")))
        })
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
        .filter_map(|e| e.metadata().ok().map(|m| (e.into_path(), m)))
        .collect()
}

fn age_days(m: &fs::Metadata, now: SystemTime) -> u64 {
    m.modified().ok().and_then(|t| now.duration_since(t).ok()).unwrap_or(Duration::ZERO).as_secs() / 86_400
}

/// Files of at least `min_size` bytes and at least `older_than_days` old, largest first.
pub fn find_large(root: &Path, min_size: u64, older_than_days: u64) -> Vec<FileInfo> {
    let now = SystemTime::now();
    let mut out: Vec<FileInfo> = walk_files(root)
        .into_iter()
        .map(|(path, m)| FileInfo { path, size: m.len(), age_days: age_days(&m, now) })
        .filter(|f| f.size >= min_size && f.age_days >= older_than_days)
        .collect();
    out.sort_by_key(|f| std::cmp::Reverse(f.size));
    out
}

#[derive(Debug, Serialize)]
pub struct DupeGroup {
    pub size: u64,
    /// Oldest first: index 0 is the copy that is kept when extras are removed.
    pub files: Vec<FileInfo>,
}

impl DupeGroup {
    pub fn wasted(&self) -> u64 {
        self.size * (self.files.len() as u64 - 1)
    }
}

fn hash_file(path: &Path, limit: Option<usize>) -> Option<blake3::Hash> {
    let mut f = File::open(path).ok()?;
    let mut hasher = blake3::Hasher::new();
    let mut buf = [0u8; 64 * 1024];
    let mut remaining = limit.unwrap_or(usize::MAX);
    while remaining > 0 {
        let want = buf.len().min(remaining);
        let n = f.read(&mut buf[..want]).ok()?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        remaining -= n;
    }
    Some(hasher.finalize())
}

/// Groups byte-identical files: by size, then a 4 KB prefix hash, then a full hash.
pub fn find_dupes(root: &Path, min_size: u64) -> Vec<DupeGroup> {
    let now = SystemTime::now();
    let mut by_size: HashMap<u64, Vec<(PathBuf, fs::Metadata)>> = HashMap::new();
    for (p, m) in walk_files(root) {
        if m.len() >= min_size.max(1) {
            by_size.entry(m.len()).or_default().push((p, m));
        }
    }
    let candidates: Vec<(u64, Vec<(PathBuf, fs::Metadata)>)> = by_size.into_iter().filter(|(_, v)| v.len() > 1).collect();

    let mut groups: Vec<DupeGroup> = candidates
        .into_par_iter()
        .flat_map(|(size, files)| {
            let split = |files: Vec<(PathBuf, fs::Metadata)>, limit: Option<usize>| {
                let mut m: HashMap<blake3::Hash, Vec<(PathBuf, fs::Metadata)>> = HashMap::new();
                for f in files {
                    if let Some(h) = hash_file(&f.0, limit) {
                        m.entry(h).or_default().push(f);
                    }
                }
                m.into_values().filter(|v| v.len() > 1).collect::<Vec<_>>()
            };
            split(files, Some(4096))
                .into_iter()
                .flat_map(|g| split(g, None))
                .map(|mut g| {
                    g.sort_by_key(|(_, m)| m.modified().unwrap_or(SystemTime::UNIX_EPOCH));
                    DupeGroup {
                        size,
                        files: g.into_iter().map(|(path, m)| FileInfo { path, size, age_days: age_days(&m, now) }).collect(),
                    }
                })
                .collect::<Vec<_>>()
        })
        .collect();
    groups.sort_by_key(|g| std::cmp::Reverse(g.wasted()));
    groups
}

/// Plan that removes every copy except the oldest in each group. Only built on explicit request.
pub fn extras_plan(groups: &[DupeGroup]) -> Plan {
    let mut plan = Plan::default();
    for g in groups {
        for f in &g.files[1..] {
            plan.items.push(Item {
                path: f.path.clone(),
                size: f.size,
                category: Category::Dupes,
                reason: format!("duplicate of {}", g.files[0].path.display()),
            });
        }
    }
    plan
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(root: &Path, rel: &str, bytes: &[u8]) {
        let p = root.join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, bytes).unwrap();
    }

    #[test]
    fn large_respects_threshold_and_skips_library() {
        let d = tempfile::tempdir().unwrap();
        write(d.path(), "big.bin", &[0; 500]);
        write(d.path(), "small.bin", &[0; 5]);
        write(d.path(), "Library/huge.bin", &[0; 900]);
        let found = find_large(d.path(), 100, 0);
        assert_eq!(found.len(), 1);
        assert!(found[0].path.ends_with("big.bin"));
        assert!(find_large(d.path(), 100, 30).is_empty());
    }

    #[test]
    fn dupes_need_identical_content_not_just_size() {
        let d = tempfile::tempdir().unwrap();
        write(d.path(), "a/one", &[1; 10_000]);
        write(d.path(), "b/two", &[1; 10_000]);
        write(d.path(), "c/three", &[2; 10_000]);
        let mut tail = vec![1u8; 10_000];
        tail[9_999] = 9; // same size and same 4 KB prefix, different tail
        write(d.path(), "d/four", &tail);
        let groups = find_dupes(d.path(), 0);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].files.len(), 2);
        assert_eq!(groups[0].wasted(), 10_000);
    }

    #[test]
    fn extras_plan_keeps_one_copy() {
        let d = tempfile::tempdir().unwrap();
        write(d.path(), "x", &[7; 50]);
        write(d.path(), "y", &[7; 50]);
        write(d.path(), "z", &[7; 50]);
        let groups = find_dupes(d.path(), 0);
        let plan = extras_plan(&groups);
        assert_eq!(plan.items.len(), 2);
        assert!(!plan.items.iter().any(|i| i.path == groups[0].files[0].path));
    }
}
