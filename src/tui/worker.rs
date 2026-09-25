//! Runs effects on background threads and reports back as messages, so the UI never blocks.

use std::sync::mpsc::Sender;
use std::thread;

use super::Ctx;
use super::app::{Effect, Msg, ScanKind, ScanResult};
use crate::core::executor;
use crate::core::plan::{Category, Item, Plan};
use crate::modules::{self, apps, large, leftovers};
use crate::platform;

pub fn spawn(effect: Effect, ctx: &Ctx, tx: &Sender<Msg>) {
    let (ctx, tx) = (ctx.clone(), tx.clone());
    thread::spawn(move || {
        for msg in run(effect, &ctx) {
            if tx.send(msg).is_err() {
                break;
            }
        }
    });
}

fn run(effect: Effect, ctx: &Ctx) -> Vec<Msg> {
    match effect {
        Effect::Scan(kind) => scan(kind, ctx),
        Effect::PlanUninstall(app) => vec![Msg::Planned(plan_uninstall(&app, ctx))],
        Effect::Execute { plan, mode, apps } => {
            let guard = if apps { &ctx.app_guard } else { &ctx.guard };
            let log = ctx.home.join(".local/state/klinit/actions.log");
            let report = executor::execute(&plan, guard, mode, Some(&log));
            vec![Msg::Executed(report, mode), Msg::Disk(platform::disk_usage(&ctx.home))]
        }
    }
}

fn scan(kind: ScanKind, ctx: &Ctx) -> Vec<Msg> {
    let list = |k, plan| vec![Msg::Scanned(ScanResult::List(k, plan))];
    match kind {
        ScanKind::Cleanup => {
            vec![Msg::Scanned(ScanResult::Cleanup(modules::scan_all(&ctx.home))), Msg::Disk(platform::disk_usage(&ctx.home))]
        }
        ScanKind::Apps => vec![Msg::Scanned(ScanResult::Apps(apps::discover(&ctx.app_dirs)))],
        ScanKind::Leftovers => {
            let ids = apps::discover(&ctx.app_dirs).into_iter().filter_map(|a| a.bundle_id).collect();
            list(kind, leftovers::scan(&ctx.home, &ids))
        }
        ScanKind::Large => {
            let items = large::find_large(&ctx.home, 100_000_000, 0)
                .into_iter()
                .take(500)
                .map(|f| Item {
                    path: f.path,
                    size: f.size,
                    category: Category::Large,
                    reason: format!("{} days since modified", f.age_days),
                })
                .collect();
            list(kind, Plan { items, warnings: vec![] })
        }
        ScanKind::Dupes => list(kind, large::extras_plan(&large::find_dupes(&ctx.home, 100_000))),
    }
}

/// Same checks as `klinit uninstall`: no running apps, no system apps, name-only matches are not removed.
fn plan_uninstall(app: &apps::App, ctx: &Ctx) -> Result<Plan, String> {
    if app.is_running(&platform::running_executables()) {
        return Err(format!("{} is running; quit it first.", app.name));
    }
    let up = apps::uninstall_plan(app, &ctx.home).map_err(|e| e.to_string())?;
    let mut plan = up.plan;
    for f in &up.fuzzy {
        plan.warnings.push(format!("not removed (name match only): {}", f.path.display()));
    }
    Ok(plan)
}
