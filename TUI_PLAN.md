# klinit: Full-screen TUI plan

Goal: `klinit` with no subcommand opens a mouse-clickable, bashtop-style dashboard. Every existing subcommand keeps working for scripts and `--json`.

## Principles

- **Reuse, don't fork.** The TUI calls the same functions the CLI does (`modules::scan_all`, `apps::discover`, `leftovers::scan`, `large::find_large`, `large::find_dupes`, `apps::uninstall_plan`) and the same `core::executor::execute`. No cleanup logic goes into the UI.
- **Safety unchanged.** Deletion only happens through `executor::execute` behind `Guard`. The TUI defaults to Trash mode and always shows a confirmation dialog with the item count and total size before acting. Permanent delete needs an explicit toggle plus a second confirmation.
- **Never block the UI thread.** All scans run on worker threads and report back over a channel.
- **Keyboard and mouse are equal.** Every clickable thing has a key binding, and the reverse.
- **Terminal safety.** Restore the terminal on panic and on Ctrl-C (panic hook plus `ratatui::restore`).

## Dependencies

Already present: `ratatui` 0.30, which re-exports `crossterm`, `rayon` and `trash`. Nothing new is required for v1.
Optional later: `sysinfo` for a real disk-usage gauge, if `statvfs` through `libc` proves awkward.

## Module layout

`src/tui.rs` (the current checklist) moves into a directory. The checklist survives as a helper used by `--interactive`.

```
src/tui/
  mod.rs          pub fn run(home) -> Result<()>; pub fn select(...) (existing checklist API, unchanged)
  app.rs          App state, Screen enum, update(Msg) -> Option<Effect>
  event.rs        Event loop: merges key/mouse/resize/tick + worker messages into Msg
  worker.rs       Spawns scans on threads, sends Msg::ScanDone(..) / Msg::Progress(..)
  hit.rs          HitMap: Vec<(Rect, Action)> rebuilt every draw, used to resolve clicks
  theme.rs        Colors, gradient helper, box-drawing styles (rounded borders)
  screens/
    dashboard.rs  Home: disk gauge, category bars, quick actions
    apps.rs       App list + detail pane + uninstall
    cleanup.rs    Category scan results (scan/clean)
    leftovers.rs  Orphan list
    large.rs      Large/old files
    dupes.rs      Duplicate groups
  widgets/
    checklist.rs  Ticked-item list (shared by cleanup/leftovers/dupes)
    bar.rs        Gradient horizontal bar with size label
    tabs.rs       Clickable tab strip
    dialog.rs     Modal confirm/result dialog
    statusbar.rs  Key hints, selected count and size, buttons
```

`main.rs` change: make `command` an `Option<Command>`; `None` calls `tui::run(home_dir()?)`. Keep `Pick::Interactive` on `tui::select`.

## Architecture (Elm-style)

```
        crossterm events ─┐
        worker messages  ─┼─► Msg ─► App::update ─► (state, Option<Effect>)
        tick (100 ms)    ─┘                              │
                                                         ├─► Effect::Scan(kind)   → worker thread
                                                         └─► Effect::Execute(plan) → worker thread
        App state ─► draw(frame) ─► fills HitMap ─► next mouse click resolves via HitMap
```

- `Msg`: `Key`, `Mouse`, `Resize`, `Tick`, `Progress`, `ScanDone(ScanResult)`, `ExecDone(Report)`.
- `Effect` keeps `update` pure and testable. The event loop runs effects.
- Each screen holds its own `Loading | Ready(data) | Error` state, so switching tabs never re-scans unless the user presses refresh.
- **Clicks** work by having every draw function register `(Rect, Action)` into the `HitMap` (tab, row, checkbox, button, dialog option). A left click finds the topmost rect containing the point and dispatches its `Action`. This keeps hit-testing next to the rendering code and avoids hand-computed coordinates.
- Mouse capture is enabled on start (`EnableMouseCapture`) and disabled on exit. Scroll wheel scrolls lists. Double-click on a row opens its detail.

## Visual design (bashtop feel)

- Rounded bordered panels with the title inset in the top border, e.g. `╭─ Apps ─────╮`.
- Truecolor, with a fallback to 16 colors when `COLORTERM` isn't `truecolor`/`24bit`.
- Size bars use a green → yellow → red gradient by share of the largest item.
- Header row: clickable tabs `Dashboard  Apps  Cleanup  Leftovers  Large  Dupes` plus the disk usage gauge on the right.
- Dashboard layout:

```
╭─ klinit ──────────────────────────────────────────────╮
│ Dashboard  Apps  Cleanup  Leftovers  Large  Dupes     │
╰───────────────────────────────────────────────────────╯
╭─ Disk ────────────────╮ ╭─ Reclaimable ───────────────╮
│ ███████████░░░░ 72%   │ │ caches   ██████████ 4.2 GB  │
│ 180 GB / 250 GB       │ │ xcode    █████      2.1 GB  │
╰───────────────────────╯ │ logs     █          310 MB  │
╭─ Quick actions ───────╮ │ trash    ▌           40 MB  │
│ [ Scan all ] [ Clean ]│ ╰─────────────────────────────╯
╰───────────────────────╯
 q quit  tab next  r refresh  ? help          mouse: on
```

- Apps screen: left list (name, size, bar), right detail pane (bundle ID, path, leftovers found, total reclaimable), `[ Uninstall ]` button.
- Spinner and progress text in the panel title while a scan runs (replaces `indicatif` inside the TUI only).
- Consistent status bar: `N selected, X GB` plus the key hints for the current screen.

## Key and mouse map

| Input | Action |
|---|---|
| `Tab` / `Shift-Tab` / `1`–`6` / click tab | Switch screen |
| `↑ ↓ j k` / scroll wheel | Move selection |
| `Space` / click checkbox | Toggle item |
| `a` | Select all / none |
| `Enter` / double-click | Open detail or confirm dialog |
| `r` | Rescan current screen |
| `d` | Toggle Trash / Permanent (with warning) |
| `?` | Help overlay |
| `q` / `Esc` | Close dialog, then quit |

## Milestones

Each milestone ends in something runnable and is committed separately.

1. **T1 Skeleton.** Move `tui.rs` into `tui/mod.rs`. Add terminal setup and teardown with a panic hook, the event loop, `Msg`/`Effect`, tab strip, empty screens and the status bar. `klinit` with no args opens it. Mouse capture on.
2. **T2 HitMap and widgets.** `HitMap`, clickable tabs, `bar`, `checklist`, `dialog`. Unit tests for hit resolution.
3. **T3 Apps screen.** Background `apps::discover`, list plus detail, uninstall flow (running-app check, `uninstall_plan`, confirm dialog, executor, result dialog). This is the first end-to-end delete path, so it gets the most testing.
4. **T4 Cleanup and Dashboard.** `scan_all` results as category bars, checklist of items, clean flow. Disk gauge on the dashboard.
5. **T5 Leftovers, Large, Dupes.** Reuse the checklist. Large is report-only with a "move to Trash" action on the selected file. Dupes shows groups and keeps the oldest, matching `extras_plan`.
6. **T6 Polish.** Help overlay, theme fallback for 16-color terminals, config options (`tui.mouse`, `tui.theme`), README section, `--no-tui` and `KLINIT_NO_MOUSE` escape hatches, completions update.

## Testing

- **Pure logic:** `App::update` tested with synthetic `Msg`s (tab switch, toggle, select all, dialog confirm/cancel). No terminal needed.
- **Rendering:** ratatui's `TestBackend` for snapshot-style checks on each screen, including a narrow terminal (e.g. 60×20) and an empty-state case.
- **Hit-testing:** feed a `Mouse` message at known coordinates after a draw and assert the resulting `Action`.
- **Safety:** an integration test with `KLINIT_HOME` on a tempdir confirms that the TUI delete path goes through `Guard` (protected paths are refused) and that dry-run and Trash mode behave as the CLI does.
- **Manual pass** in Terminal.app, iTerm2 and Ghostty/Alacritty for mouse and color behavior.

## Risks and open questions

- **Terminal.app** has no truecolor, so the 16/256-color fallback needs to look acceptable, not just work.
- **Scan cost.** `find_dupes` and `find_large` over `~` are slow. Default the TUI to a narrower root, let the user change it, and support cancelling a running scan.
- **Full Disk Access warnings** (`Plan.warnings`) need a visible place, likely a warnings panel or a status-bar badge.
- **Scope of v1.** Decide whether Large and Dupes ship in the first release or follow after Apps, Cleanup and Leftovers.
- **Trash of `/Applications` items** may need permissions the TUI can't prompt for. Surface the executor's error in the result dialog rather than failing silently.

## Status

T1 to T5 are implemented, plus the help overlay, 16-color fallback and `KLINIT_NO_MOUSE` from T6. Differences from the plan above:

- The screens and widgets live in `view.rs` and the state in `app.rs`, not one file per screen.
- Large files and Duplicates use the same checklist as Cleanup; Large offers "move to Trash" on ticked files.
- A running scan cannot be cancelled (`r` is ignored while one runs); `find_large`/`find_dupes` have no cancellation hook.
- The `tui.mouse` / `tui.theme` config options and a `--no-tui` flag were not added. Running without a terminal prints help.
