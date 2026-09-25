//! Cleanup locations as data: macOS paths shift between versions, so they live in one table.

use crate::core::plan::Category;

#[derive(Clone, Copy)]
pub enum Mode {
    /// Each child of the directory is an item; the directory itself is kept.
    Contents,
    /// The path itself is the item.
    Whole,
}

pub struct Target {
    pub category: Category,
    pub rel_path: &'static str,
    pub mode: Mode,
    /// Child names (Contents mode) claimed by another category, so nothing is counted twice.
    pub skip: &'static [&'static str],
    pub reason: &'static str,
}

const fn t(category: Category, rel_path: &'static str, mode: Mode, reason: &'static str) -> Target {
    Target { category, rel_path, mode, skip: &[], reason }
}

pub const CATEGORIES: &[(Category, &str)] = &[
    (Category::Caches, "Application caches in ~/Library/Caches"),
    (Category::Logs, "Application and crash logs"),
    (Category::Trash, "Files in the Trash (not recoverable)"),
    (Category::Xcode, "Xcode derived data and simulator caches"),
    (Category::Browsers, "Safari, Chrome and Firefox caches (never profiles or history)"),
    (Category::Dev, "Package manager and build tool caches"),
];

pub const TARGETS: &[Target] = &[
    Target {
        category: Category::Caches,
        rel_path: "Library/Caches",
        mode: Mode::Contents,
        skip: &["com.apple.Safari", "Google", "Firefox", "Yarn", "Homebrew"],
        reason: "application cache",
    },
    t(Category::Logs, "Library/Logs", Mode::Contents, "application log"),
    t(Category::Logs, "Library/Application Support/CrashReporter", Mode::Contents, "crash report"),
    t(Category::Trash, ".Trash", Mode::Contents, "in Trash"),
    t(Category::Xcode, "Library/Developer/Xcode/DerivedData", Mode::Contents, "Xcode derived data"),
    t(Category::Xcode, "Library/Developer/Xcode/iOS DeviceSupport", Mode::Contents, "iOS device support files"),
    t(Category::Xcode, "Library/Developer/CoreSimulator/Caches", Mode::Contents, "simulator cache"),
    t(Category::Browsers, "Library/Caches/com.apple.Safari", Mode::Whole, "Safari cache"),
    t(Category::Browsers, "Library/Caches/Google/Chrome", Mode::Whole, "Chrome cache"),
    t(Category::Browsers, "Library/Caches/Firefox", Mode::Whole, "Firefox cache"),
    t(Category::Dev, ".npm/_cacache", Mode::Whole, "npm cache"),
    t(Category::Dev, "Library/Caches/Yarn", Mode::Whole, "Yarn cache"),
    t(Category::Dev, ".cargo/registry/cache", Mode::Whole, "Cargo registry cache"),
    t(Category::Dev, "Library/Caches/Homebrew", Mode::Whole, "Homebrew download cache"),
    t(Category::Dev, ".gradle/caches", Mode::Whole, "Gradle cache"),
];
