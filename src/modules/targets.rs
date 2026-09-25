//! Cleanup locations as data: macOS paths shift between versions, so they live in one table.

#[derive(Clone, Copy)]
pub enum Mode {
    /// Each child of the directory is an item; the directory itself is kept.
    Contents,
    /// The path itself is the item.
    Whole,
}

pub struct Target {
    pub category: &'static str,
    pub rel_path: &'static str,
    pub mode: Mode,
    /// Child names (Contents mode) claimed by another category, so nothing is counted twice.
    pub skip: &'static [&'static str],
    pub reason: &'static str,
}

const fn t(category: &'static str, rel_path: &'static str, mode: Mode, reason: &'static str) -> Target {
    Target { category, rel_path, mode, skip: &[], reason }
}

pub const CATEGORIES: &[(&str, &str)] = &[
    ("caches", "Application caches in ~/Library/Caches"),
    ("logs", "Application and crash logs"),
    ("trash", "Files in the Trash (not recoverable)"),
    ("xcode", "Xcode derived data and simulator caches"),
    ("browsers", "Safari, Chrome and Firefox caches (never profiles or history)"),
    ("dev", "Package manager and build tool caches"),
];

pub const TARGETS: &[Target] = &[
    Target {
        category: "caches",
        rel_path: "Library/Caches",
        mode: Mode::Contents,
        skip: &["com.apple.Safari", "Google", "Firefox", "Yarn", "Homebrew"],
        reason: "application cache",
    },
    t("logs", "Library/Logs", Mode::Contents, "application log"),
    t("logs", "Library/Application Support/CrashReporter", Mode::Contents, "crash report"),
    t("trash", ".Trash", Mode::Contents, "in Trash"),
    t("xcode", "Library/Developer/Xcode/DerivedData", Mode::Contents, "Xcode derived data"),
    t("xcode", "Library/Developer/Xcode/iOS DeviceSupport", Mode::Contents, "iOS device support files"),
    t("xcode", "Library/Developer/CoreSimulator/Caches", Mode::Contents, "simulator cache"),
    t("browsers", "Library/Caches/com.apple.Safari", Mode::Whole, "Safari cache"),
    t("browsers", "Library/Caches/Google/Chrome", Mode::Whole, "Chrome cache"),
    t("browsers", "Library/Caches/Firefox", Mode::Whole, "Firefox cache"),
    t("dev", ".npm/_cacache", Mode::Whole, "npm cache"),
    t("dev", "Library/Caches/Yarn", Mode::Whole, "Yarn cache"),
    t("dev", ".cargo/registry/cache", Mode::Whole, "Cargo registry cache"),
    t("dev", "Library/Caches/Homebrew", Mode::Whole, "Homebrew download cache"),
    t("dev", ".gradle/caches", Mode::Whole, "Gradle cache"),
];
