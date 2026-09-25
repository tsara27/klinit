use std::path::PathBuf;

use serde::{Serialize, Serializer};

use super::scanner::path_size;

/// What kind of thing an item is. The names are user-visible (CLI arguments and JSON output).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    // Cleanup categories, selectable by name.
    Caches,
    Logs,
    Trash,
    Xcode,
    Browsers,
    Dev,
    // Other item kinds.
    App,
    /// Data matched to an uninstalled app by exact bundle ID.
    Leftover,
    /// A folder that merely shares an app's name.
    Fuzzy,
    /// Data whose bundle ID matches no installed app.
    Orphaned,
    Large,
    Dupes,
}

impl Category {
    pub fn as_str(self) -> &'static str {
        match self {
            Category::Caches => "caches",
            Category::Logs => "logs",
            Category::Trash => "trash",
            Category::Xcode => "xcode",
            Category::Browsers => "browsers",
            Category::Dev => "dev",
            Category::App => "app",
            Category::Leftover => "leftover",
            Category::Fuzzy => "fuzzy",
            Category::Orphaned => "leftovers",
            Category::Large => "large",
            Category::Dupes => "dupes",
        }
    }

    /// Whether items of this kind may be deleted without going through the Trash.
    /// Caches, logs and files the user picked by hand are fine; app bundles and their data
    /// cannot be regenerated, so those always go to the Trash where they can be restored.
    pub fn allows_permanent(self) -> bool {
        !matches!(self, Category::App | Category::Leftover | Category::Fuzzy | Category::Orphaned)
    }

    /// Emptying the Trash is irreversible, so bulk selections leave it out.
    pub fn is_trash(self) -> bool {
        self == Category::Trash
    }
}

impl std::fmt::Display for Category {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.pad(self.as_str())
    }
}

impl Serialize for Category {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.as_str())
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Item {
    pub path: PathBuf,
    pub size: u64,
    pub category: Category,
    pub reason: String,
}

impl Item {
    /// Sizes `path` without following symlinks; a path that cannot be read counts as 0 bytes.
    pub fn from_path(path: PathBuf, category: Category, reason: impl Into<String>) -> Self {
        let size = path_size(&path).unwrap_or(0);
        Item { path, size, category, reason: reason.into() }
    }
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
    /// Items that may not be deleted permanently (see `Category::allows_permanent`).
    pub fn trash_only(&self) -> impl Iterator<Item = &Item> {
        self.items.iter().filter(|i| !i.category.allows_permanent())
    }

    pub fn total_size(&self) -> u64 {
        self.items.iter().map(|i| i.size).sum()
    }
}
