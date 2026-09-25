mod config;
mod core;
mod modules;
mod platform;
mod report;
mod tui;

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

use modules::apps;
use core::executor::{self, Mode};
use core::safety::Guard;
use core::scanner::{child_sizes, format_size};

#[derive(Parser)]
#[command(name = "klinit", version, about = "Clean up your Mac from the command line")]
struct Cli {
    /// Without a subcommand, opens the interactive full-screen interface.
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Show what is using space under a directory (default: home)
    Disk {
        path: Option<PathBuf>,
        /// Number of entries to show
        #[arg(short = 'n', long, default_value_t = 15)]
        top: usize,
        #[arg(long)]
        json: bool,
    },
    /// Show reclaimable space per category
    Scan {
        #[arg(long)]
        json: bool,
    },
    /// Clean categories (dry run unless --yes): caches, logs, trash, xcode, browsers, dev, all
    Clean {
        /// Categories to clean; falls back to `default_categories` from the config file
        categories: Vec<String>,
        /// Pick items from a checklist before acting
        #[arg(short, long)]
        interactive: bool,
        /// Actually delete (moves to Trash)
        #[arg(long)]
        yes: bool,
        /// With --yes, delete permanently instead of using the Trash
        #[arg(long)]
        permanent: bool,
        #[arg(long)]
        json: bool,
    },
    /// Uninstall an app and its leftovers (dry run unless --yes)
    Uninstall {
        /// App name, file name or bundle ID
        app: String,
        #[arg(long)]
        yes: bool,
        #[arg(long)]
        permanent: bool,
        /// Also remove folders that only match the app's name (not its bundle ID)
        #[arg(long)]
        include_fuzzy: bool,
        #[arg(long)]
        json: bool,
    },
    /// Orphaned support files from apps that are no longer installed (report only unless --remove)
    Leftovers {
        /// Build a removal plan (still a dry run unless --yes)
        #[arg(long)]
        remove: bool,
        /// Pick items from a checklist (implies --remove)
        #[arg(short, long)]
        interactive: bool,
        #[arg(long)]
        yes: bool,
        #[arg(long)]
        permanent: bool,
        #[arg(long)]
        json: bool,
    },
    /// Find large or old files (report only)
    Large {
        path: Option<PathBuf>,
        /// Minimum size in MB
        #[arg(long, default_value_t = 100)]
        min_mb: u64,
        /// Only files not modified for this many days
        #[arg(long, default_value_t = 0)]
        older_than: u64,
        #[arg(short = 'n', long, default_value_t = 30)]
        top: usize,
        #[arg(long)]
        json: bool,
    },
    /// Find duplicate files (report only unless --remove-extras, which keeps the oldest copy)
    Dupes {
        path: Option<PathBuf>,
        /// Minimum size in KB
        #[arg(long, default_value_t = 100)]
        min_kb: u64,
        #[arg(long)]
        remove_extras: bool,
        /// Pick items from a checklist (implies --remove-extras)
        #[arg(short, long)]
        interactive: bool,
        #[arg(long)]
        yes: bool,
        #[arg(long)]
        permanent: bool,
        #[arg(long)]
        json: bool,
    },
    /// Show the config file path and effective settings
    Config,
    /// Print a shell completion script
    Completions { shell: clap_complete::Shell },
    /// List installed apps with size
    Apps {
        #[arg(long)]
        json: bool,
    },
}

/// `KLINIT_HOME` overrides the home directory so tests never touch the real one.
fn home_dir() -> Result<PathBuf> {
    std::env::var_os("KLINIT_HOME")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .context("cannot determine home directory")
}

/// System-wide and per-user application folders.
fn app_dirs(home: &std::path::Path) -> Vec<PathBuf> {
    vec![PathBuf::from("/Applications"), home.join("Applications")]
}

/// How a plan is confirmed: everything, or picked from a checklist.
#[derive(Clone, Copy)]
enum Pick {
    All,
    /// Checklist; the flag says whether items start ticked.
    Interactive(bool),
}

fn guard_for(home: &std::path::Path) -> Result<Guard> {
    Ok(Guard::new(home)?.protect(config::load(home)?.exclude_paths(home)))
}

fn spinner(msg: &'static str, json: bool) -> indicatif::ProgressBar {
    if json || !std::io::IsTerminal::is_terminal(&std::io::stderr()) {
        return indicatif::ProgressBar::hidden();
    }
    let pb = indicatif::ProgressBar::new_spinner().with_message(msg);
    pb.enable_steady_tick(std::time::Duration::from_millis(100));
    pb
}

fn run_plan(plan: &core::plan::Plan, guard: Guard, yes: bool, permanent: bool, json: bool) -> Result<()> {
    run_plan_with(plan, guard, yes, permanent, json, Pick::All)
}

fn run_plan_with(plan: &core::plan::Plan, guard: Guard, yes: bool, permanent: bool, json: bool, pick: Pick) -> Result<()> {
    let chosen;
    let plan = match pick {
        Pick::All => plan,
        Pick::Interactive(pre) => {
            chosen = core::plan::Plan { items: tui::select(plan.items.clone(), pre)?, warnings: plan.warnings.clone() };
            &chosen
        }
    };
    let mode = match (yes, permanent) {
        (false, _) => Mode::DryRun,
        (true, false) => Mode::Trash,
        (true, true) => Mode::Permanent,
    };
    let log = guard.home().join(".local/state/klinit/actions.log");
    let result = executor::execute(plan, &guard, mode, Some(&log));
    report::print_clean(plan, &result, mode, json)
}

fn run_tui() -> Result<()> {
    if !std::io::IsTerminal::is_terminal(&std::io::stdout()) {
        return Ok(<Cli as clap::CommandFactory>::command().print_help()?);
    }
    let home = home_dir()?;
    let app_dirs = app_dirs(&home);
    let guard = guard_for(&home)?;
    let mut app_guard = guard_for(&home)?;
    for d in &app_dirs {
        app_guard = app_guard.allow_app_dir(d);
    }
    tui::run(tui::Ctx { home, app_dirs, guard: std::sync::Arc::new(guard), app_guard: std::sync::Arc::new(app_guard) })
}

fn main() -> Result<()> {
    let Some(command) = Cli::parse().command else { return run_tui() };
    match command {
        Command::Disk { path, top, json } => {
            let path = match path {
                Some(p) => p,
                None => home_dir()?,
            };
            let sizes = child_sizes(&path).with_context(|| format!("cannot read {}", path.display()))?;
            let shown = &sizes[..sizes.len().min(top)];
            if json {
                let rows: Vec<_> = shown.iter().map(|(p, s)| serde_json::json!({"path": p, "size": s})).collect();
                println!("{}", serde_json::to_string_pretty(&rows)?);
            } else {
                for (p, s) in shown {
                    println!("{:>10}  {}", format_size(*s), p.display());
                }
                println!("{:>10}  total", format_size(sizes.iter().map(|(_, s)| s).sum()));
            }
        }
        Command::Scan { json } => {
            let pb = spinner("scanning...", json);
            let rows = modules::scan_all(&home_dir()?);
            pb.finish_and_clear();
            report::print_scan(&rows, json)?
        }
        Command::Clean { categories, interactive, yes, permanent, json } => {
            let home = home_dir()?;
            let categories = if categories.is_empty() { config::load(&home)?.default_categories } else { categories };
            let pb = spinner("scanning...", json);
            let plan = modules::build_plan(&home, &categories)?;
            pb.finish_and_clear();
            let pick = if interactive { Pick::Interactive(true) } else { Pick::All };
            run_plan_with(&plan, guard_for(&home)?, yes, permanent, json, pick)?;
        }
        Command::Config => {
            let home = home_dir()?;
            println!("# {}", config::path(&home).display());
            print!("{}", toml::to_string_pretty(&config::load(&home)?)?);
        }
        Command::Completions { shell } => {
            clap_complete::generate(shell, &mut <Cli as clap::CommandFactory>::command(), "klinit", &mut std::io::stdout());
        }
        Command::Apps { json } => {
            let apps = apps::discover(&app_dirs(&home_dir()?));
            report::print_apps(&apps, json)?;
        }
        Command::Leftovers { remove, interactive, yes, permanent, json } => {
            let home = home_dir()?;
            let pb = spinner("scanning...", json);
            let ids = apps::discover(&app_dirs(&home)).into_iter().filter_map(|a| a.bundle_id).collect();
            let plan = modules::leftovers::scan(&home, &ids);
            pb.finish_and_clear();
            if remove || interactive {
                let pick = if interactive { Pick::Interactive(false) } else { Pick::All };
                run_plan_with(&plan, guard_for(&home)?, yes, permanent, json, pick)?;
            } else {
                report::print_plan(&plan, json)?;
                if !json {
                    println!("Report only. Review the list, then use --remove (and --yes) to clean it up.");
                }
            }
        }
        Command::Large { path, min_mb, older_than, top, json } => {
            let root = match path { Some(p) => p, None => home_dir()? };
            let pb = spinner("scanning...", json);
            let files = modules::large::find_large(&root, min_mb * 1_000_000, older_than);
            pb.finish_and_clear();
            report::print_files(&files[..files.len().min(top)], json)?;
        }
        Command::Dupes { path, min_kb, remove_extras, interactive, yes, permanent, json } => {
            let home = home_dir()?;
            let root = path.unwrap_or_else(|| home.clone());
            let pb = spinner("hashing candidates...", json);
            let groups = modules::large::find_dupes(&root, min_kb * 1000);
            pb.finish_and_clear();
            if remove_extras || interactive {
                let plan = modules::large::extras_plan(&groups);
                let pick = if interactive { Pick::Interactive(false) } else { Pick::All };
                run_plan_with(&plan, guard_for(&home)?, yes, permanent, json, pick)?;
            } else {
                report::print_dupes(&groups, json)?;
            }
        }
        Command::Uninstall { app, yes, permanent, include_fuzzy, json } => {
            let home = home_dir()?;
            let dirs = app_dirs(&home);
            let installed = apps::discover(&dirs);
            let app = apps::find(&installed, &app)?;
            if app.is_running(&platform::running_executables()) {
                anyhow::bail!("{} is running; quit it first", app.name);
            }
            let up = apps::uninstall_plan(app, &home)?;
            let mut plan = up.plan;
            if include_fuzzy {
                plan.items.extend(up.fuzzy);
            } else {
                for f in &up.fuzzy {
                    plan.warnings.push(format!("not removed (name match only): {}; use --include-fuzzy", f.path.display()));
                }
            }
            let mut guard = guard_for(&home)?;
            for d in &dirs {
                guard = guard.allow_app_dir(d);
            }
            run_plan(&plan, guard, yes, permanent, json)?;
        }
    }
    Ok(())
}
