use std::path::PathBuf;

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct Item {
    pub path: PathBuf,
    pub size: u64,
    pub category: String,
    pub reason: String,
}

/// What a module proposes to delete. Building a plan never modifies the disk.
#[derive(Debug, Default, Serialize)]
pub struct Plan {
    pub items: Vec<Item>,
    /// Non-fatal problems found while scanning, e.g. a directory that needs Full Disk Access.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

impl Plan {
    pub fn total_size(&self) -> u64 {
        self.items.iter().map(|i| i.size).sum()
    }
}
