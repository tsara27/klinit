# klinit: Plan

A CleanMyMac-style CLI for macOS, written in Rust.

## Principles

- **Dry-run by default.** Nothing is deleted without `--yes`.
- **Trash by default.** Items move to Trash. `--permanent` hard-deletes.
- **Scan and execute are separate.** Modules only produce a `Plan`. `core::executor` is the only code that deletes, and it re-checks every path through `core::safety::Guard`.
- **User-level only** (`~`), no `sudo` in v1.
- **Data-driven targets.** Cleanup locations live in a table, not scattered through code, because macOS paths shift between versions.

## Commands

```
klinit scan                 # reclaimable space by category
klinit clean [category...]  # caches, logs, trash, xcode, browser, dev
klinit trash empty          # empty Trash
klinit uninstall <App>      # app + leftovers
klinit apps                 # installed apps with size
klinit leftovers            # orphaned support files from deleted apps
klinit large [path]         # large/old files
klinit dupes [path]         # duplicate finder
klinit disk [path]          # space usage by child entry
klinit config
```

Global flags: `--dry-run` (default), `--yes`, `--permanent`, `--json`, `--verbose`.

## Layout

```
src/
  main.rs / cli.rs
  core/       scanner.rs  plan.rs  executor.rs  safety.rs
  modules/    one file per category; each returns a Plan
  platform/   macos.rs (plist, Trash, TCC)
  report.rs
```

## Safety model

- Protected: home itself, `Documents`, `Desktop`, `Pictures`, `Movies`, `Music`, `Downloads`, `.ssh`, `.gnupg`, `Library/Keychains`. Not overridable.
- Anything outside `$HOME` is rejected.
- Paths are canonicalized first, so symlinks and `..` are judged by their real target.
- `com.apple.*` apps are never uninstalled.
- Running apps and in-use files are skipped.
- Every action is appended to `~/.local/state/klinit/actions.log`.
- Tests use a temp `$HOME` (`KLINIT_HOME`). Never run destructive tests against the real home.

## Milestones

- [x] **M0: skeleton.** Cargo project, clap, parallel scanner, `klinit disk`.
- [x] **M1: safety and executor.** `Guard`, `Plan`, `execute` (dry-run/Trash/permanent), action log, unit tests.
- [x] **M2: `scan` and `clean`.** Detailed below.
- [x] **M3: `uninstall` and `apps`.** Info.plist bundle IDs, leftover discovery, running-app check.
- [ ] **M4: `leftovers`, `large`, `dupes`.** Lower-confidence features, never auto-selected.
- [ ] **M5: polish.** Interactive checklist TUI (ratatui), config file, progress bars, shell completions.
- [ ] **M6: release.** Homebrew tap, GitHub Actions universal binary, notarization.

## M2: `scan` and `clean`

### Goal

`klinit scan` shows reclaimable space per category. `klinit clean <categories>` builds a plan from those categories and runs it through the executor.

### Design

1. **`Module` trait** in `src/modules/mod.rs`:
   ```rust
   pub trait Module {
       fn id(&self) -> &'static str;        // "caches", "logs", ...
       fn description(&self) -> &'static str;
       fn scan(&self, home: &Path) -> Plan;  // read-only
   }
   ```
2. **Target table** in `src/modules/targets.rs`: static list of `Target { category, rel_path, mode, reason }` where `mode` is `Contents` (delete children, keep the dir) or `Whole`. Modules are thin wrappers over this table.
3. **Registry** `modules::all()` returns every module. `scan` runs them in parallel with rayon.
4. **`clean`** merges the plans of the selected modules, prints a summary table, then calls `execute`:
   - default: `Mode::DryRun`
   - `--yes`: `Mode::Trash`
   - `--yes --permanent`: `Mode::Permanent`
   - action log path: `~/.local/state/klinit/actions.log`
5. **Reporting** in `src/report.rs`: human table (category, items, size) and `--json` output.

### Categories and targets

| Category | Targets (relative to `~`) | Notes |
|---|---|---|
| `caches` | `Library/Caches/*` | Contents of each app cache dir |
| `logs` | `Library/Logs/*`, `Library/Application Support/CrashReporter/*` | |
| `trash` | `.Trash/*` | Volume trashes deferred; TCC error gives a Full Disk Access hint |
| `xcode` | `Library/Developer/Xcode/DerivedData/*`, `Library/Developer/Xcode/iOS DeviceSupport/*`, `Library/Developer/CoreSimulator/Caches/*` | Skip if absent |
| `browsers` | Safari, Chrome, Firefox cache dirs under `Library/Caches` and `Library/Application Support` | Cache only, never profiles, cookies, or history |
| `dev` | `.npm/_cacache`, `Library/Caches/Yarn`, `.cargo/registry/cache`, `Library/Caches/Homebrew`, `.gradle/caches` | Cache dirs only |

Out of scope for M2: `brew cleanup` and `docker system prune` (need to shell out, so they come after the executor gains a "command action" type).

### Files to add or change

- `src/modules/{mod.rs, targets.rs, caches.rs, logs.rs, trash.rs, xcode.rs, browsers.rs, dev.rs}`
- `src/report.rs`
- `src/main.rs`: add `Scan` and `Clean` subcommands (`categories: Vec<String>`, `--yes`, `--permanent`, `--json`)
- Wire the executor's current dead code into `clean`

### Tests

- Each module against a fixture home in a tempdir: correct items found, correct sizes, absent paths yield an empty plan (no error).
- A module never proposes a protected path (assert every item passes `Guard::check`).
- `clean` dry-run leaves the fixture untouched. `--permanent` removes exactly the planned items.
- Unknown category name returns an error listing valid categories.
- Manual: `klinit scan` on the real home (read-only), then `clean caches` dry-run, before any real `--yes` run.

### Done when

- `klinit scan` prints a per-category table and `--json` output.
- `klinit clean caches logs` is a dry-run by default and only deletes with `--yes`.
- All new tests pass and the M2 code produces no dead-code warnings.

## M3: `uninstall` and `apps` (done)

- `modules/apps.rs`: discovers `*.app` in `/Applications` and `~/Applications`, reads `Info.plist` (bundle ID, name, version) with the `plist` crate.
- `uninstall` matches by bundle ID, display name or file name. It refuses `com.apple.*` apps and running apps (`platform::running_executables`, path-prefix match).
- Leftovers are matched by exact bundle ID in a table (`LEFTOVER_LOCATIONS`) plus Group Containers. Name-only matches are "fuzzy": reported as warnings, removed only with `--include-fuzzy`.
- `Guard::allow_app_dir` permits deleting `*.app` bundles directly inside the app directories, the only exception to the outside-`$HOME` rule.
- Not covered: nested apps (e.g. `/Applications/Utilities`), ByHost preferences, login items, apps whose bundle is not user-writable (reported as skipped by the executor).

## Open questions

- Should `clean` with no arguments mean "all safe categories" or require explicit categories? Default proposal: require explicit categories, with `all` as an opt-in.
- Should the Trash module be excluded from `all` since it is not recoverable? Default proposal: yes, `trash empty` stays a separate command.

## Risks

- **Full Disk Access / TCC:** degrade gracefully and explain how to grant it.
- **Over-eager fuzzy matching in uninstall (M3):** exact bundle ID first, confirmation for fuzzy matches.
- **macOS path changes:** keep targets in the data table.
- **Scope creep:** malware scan, RAM optimization and login-item management are out of scope for v1.
