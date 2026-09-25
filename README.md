# klinit

A CleanMyMac-style cleanup tool for the macOS command line, written in Rust.

- **Dry run by default.** Nothing is deleted until you pass `--yes`.
- **Trash by default.** Deleted items go to the Trash; `--permanent` skips it.
- **User-level only.** It works inside your home directory (plus `.app` bundles in `/Applications`) and never needs `sudo`.
- **Protected locations.** `Documents`, `Desktop`, `Pictures`, `Movies`, `Music`, `Downloads`, `.ssh`, `.gnupg` and `Library/Keychains` are never touched, and `com.apple.*` apps are never uninstalled.
- Every deletion is logged to `~/.local/state/klinit/actions.log`.

## Install

### Homebrew

```sh
brew install <owner>/tap/klinit
```

### From a release

Download `klinit-<version>-macos-universal.tar.gz` from the GitHub releases page, then:

```sh
tar -xzf klinit-*-macos-universal.tar.gz
sudo mv klinit /usr/local/bin/
```

### From source

Requires a recent Rust toolchain.

```sh
git clone git@github.com:tsara27/klinit.git
cd klinit
cargo install --path .
```

### Full Disk Access

Some folders (such as `~/.Trash`) need your terminal to have Full Disk Access: System Settings > Privacy & Security > Full Disk Access. Without it klinit prints a warning and skips those folders.

## Usage

Every command that can delete is a dry run unless you add `--yes`. Most commands accept `--json`.

### Clean caches and junk

```sh
klinit scan                      # reclaimable space per category
klinit clean caches logs         # dry run: shows what would be removed
klinit clean caches logs --yes   # moves it to the Trash
klinit clean all --yes --permanent
klinit clean caches -i           # pick items from a checklist
```

Categories: `caches`, `logs`, `trash`, `xcode`, `browsers`, `dev`. `all` means every category except `trash`.

### Apps

```sh
klinit apps                          # installed apps with size and bundle ID
klinit uninstall Slack               # dry run: app plus its leftovers
klinit uninstall Slack --yes
klinit uninstall Slack --include-fuzzy --yes
```

`uninstall` accepts an app name, file name or bundle ID. It refuses to run if the app is open. Leftovers are matched by exact bundle ID. Folders that only match the app's name are listed as warnings and removed only with `--include-fuzzy`.

### Find things to delete by hand

These are lower confidence, so they only report unless you opt in.

```sh
klinit leftovers                   # support files from apps no longer installed
klinit leftovers --remove --yes    # or -i to choose from a checklist

klinit large ~/Movies --min-mb 500 --older-than 180
klinit dupes ~/Pictures --min-kb 100
klinit dupes ~/Pictures --remove-extras --yes   # keeps the oldest copy in each group
```

### Disk usage

```sh
klinit disk            # largest entries in your home directory
klinit disk ~/Library -n 30
```

### Interactive checklist

`-i` / `--interactive` on `clean`, `leftovers` and `dupes` opens a checklist:

| Key | Action |
|---|---|
| `↑` `↓` / `j` `k` | Move |
| `space` | Toggle item |
| `a` | Select all / none |
| `enter` | Confirm |
| `q` / `esc` | Cancel |

It still needs `--yes` to actually delete.

### Full-screen interface

Run `klinit` with no arguments to open a dashboard with tabs for Apps, Cleanup, Leftovers, Large files and Duplicates. Everything is clickable (tabs, rows, checkboxes, buttons) and also works from the keyboard: `tab`/`1`-`6` switch screens, `space` ticks, `a` selects all, `enter` acts on the selection, `r` rescans, `d` toggles Trash/permanent, `?` shows help, `q` quits.

Nothing is deleted without a confirmation dialog, items go to the Trash unless you switch to permanent (which asks twice), and deletion uses the same safety checks as the subcommands. Set `KLINIT_NO_MOUSE=1` to turn mouse capture off. Piping the output (not a terminal) prints the usual help instead.

### Shell completions

```sh
klinit completions zsh > ~/.zfunc/_klinit      # also: bash, fish, elvish, powershell
```

Homebrew installs completions for you.

## Configuration

Optional file at `~/.config/klinit/config.toml`:

```toml
# Paths klinit must never touch (relative to your home, or absolute).
exclude = ["Projects", "Library/Application Support/MyApp"]

# Categories used when you run `klinit clean` with no arguments.
default_categories = ["caches", "logs"]
```

`klinit config` prints the path and the settings in effect. A malformed file is an error, so an exclusion is never silently dropped.

## Safety notes

- Run a dry run first and read the list.
- Paths are resolved through symlinks and `..` before checking, so a link cannot smuggle a protected folder into a plan.
- Running apps are not uninstalled. Cleaning caches while an app is open is allowed, so quit apps first if you want to be careful.
- Trash is recoverable; `--permanent` is not.

## Development

```sh
cargo test
cargo clippy -- -D warnings
```

Tests run against temporary directories. Set `KLINIT_HOME` to point klinit at a fake home when experimenting:

```sh
KLINIT_HOME=/tmp/fakehome klinit scan
```

See [PLAN.md](PLAN.md) for the design and milestones.
