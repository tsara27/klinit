mod core;

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

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
}

/// `KLINIT_HOME` overrides the home directory so tests never touch the real one.
fn home_dir() -> Result<PathBuf> {
    std::env::var_os("KLINIT_HOME")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .context("cannot determine home directory")
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
    }
    Ok(())
}
