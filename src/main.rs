mod core;
mod modules;
mod platform;
mod report;

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
    #[command(subcommand)]
    command: Command,
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
        categories: Vec<String>,
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

fn run_plan(plan: &core::plan::Plan, guard: Guard, yes: bool, permanent: bool, json: bool) -> Result<()> {
    let mode = match (yes, permanent) {
        (false, _) => Mode::DryRun,
        (true, false) => Mode::Trash,
        (true, true) => Mode::Permanent,
    };
    let log = guard.home().join(".local/state/klinit/actions.log");
    let result = executor::execute(plan, &guard, mode, Some(&log));
    report::print_clean(plan, &result, mode, json)
}

fn main() -> Result<()> {
    match Cli::parse().command {
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
        Command::Scan { json } => report::print_scan(&modules::scan_all(&home_dir()?), json)?,
        Command::Clean { categories, yes, permanent, json } => {
            let home = home_dir()?;
            let plan = modules::build_plan(&home, &categories)?;
            run_plan(&plan, Guard::new(&home)?, yes, permanent, json)?;
        }
        Command::Apps { json } => {
            let apps = apps::discover(&app_dirs(&home_dir()?));
            report::print_apps(&apps, json)?;
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
            let mut guard = Guard::new(&home)?;
            for d in &dirs {
                guard = guard.allow_app_dir(d);
            }
            run_plan(&plan, guard, yes, permanent, json)?;
        }
    }
    Ok(())
}
