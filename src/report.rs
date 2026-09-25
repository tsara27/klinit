use crate::core::executor::{Mode, Report};
use crate::core::plan::Plan;
use crate::core::scanner::format_size;
use crate::modules::apps::App;

pub fn print_scan(rows: &[(&str, &str, Plan)], json: bool) -> anyhow::Result<()> {
    if json {
        let out: Vec<_> = rows
            .iter()
            .map(|(id, desc, p)| serde_json::json!({"category": id, "description": desc, "items": p.items.len(), "size": p.total_size(), "warnings": p.warnings}))
            .collect();
        println!("{}", serde_json::to_string_pretty(&out)?);
        return Ok(());
    }
    println!("{:<10} {:>6} {:>10}  {}", "CATEGORY", "ITEMS", "SIZE", "DESCRIPTION");
    for (id, desc, p) in rows {
        println!("{:<10} {:>6} {:>10}  {}", id, p.items.len(), format_size(p.total_size()), desc);
    }
    let total: u64 = rows.iter().filter(|(id, ..)| *id != "trash").map(|(.., p)| p.total_size()).sum();
    println!("{:<10} {:>6} {:>10}  (excluding trash)", "total", "", format_size(total));
    for w in rows.iter().flat_map(|(.., p)| &p.warnings) {
        eprintln!("warning: {w}");
    }
    Ok(())
}

pub fn print_clean(plan: &Plan, report: &Report, mode: Mode, json: bool) -> anyhow::Result<()> {
    if json {
        let out = serde_json::json!({
            "mode": format!("{mode:?}"),
            "items": plan.items,
            "removed": report.removed.len(),
            "freed": report.freed(),
            "skipped": report.skipped.iter().map(|(p, why)| serde_json::json!({"path": p, "reason": why})).collect::<Vec<_>>(),
            "warnings": plan.warnings,
        });
        println!("{}", serde_json::to_string_pretty(&out)?);
        return Ok(());
    }
    let mut items: Vec<_> = plan.items.iter().collect();
    items.sort_by(|a, b| b.size.cmp(&a.size));
    for i in items.iter().take(20) {
        println!("{:>10}  [{}] {}", format_size(i.size), i.category, i.path.display());
    }
    if items.len() > 20 {
        println!("           ... and {} more", items.len() - 20);
    }
    for (p, why) in &report.skipped {
        eprintln!("skipped {}: {why}", p.display());
    }
    for w in &plan.warnings {
        eprintln!("warning: {w}");
    }
    let n = report.removed.len();
    let size = format_size(report.freed());
    match mode {
        Mode::DryRun => println!("Dry run: {n} items ({size}) would be removed. Re-run with --yes to move them to the Trash."),
        Mode::Trash => println!("Moved {n} items ({size}) to the Trash."),
        Mode::Permanent => println!("Permanently deleted {n} items ({size})."),
    }
    Ok(())
}

pub fn print_apps(apps: &[App], json: bool) -> anyhow::Result<()> {
    if json {
        let out: Vec<_> = apps
            .iter()
            .map(|a| serde_json::json!({"name": a.name, "bundle_id": a.bundle_id, "version": a.version, "size": a.size, "path": a.path}))
            .collect();
        println!("{}", serde_json::to_string_pretty(&out)?);
        return Ok(());
    }
    println!("{:>10}  {:<32} {}", "SIZE", "NAME", "BUNDLE ID");
    for a in apps {
        println!("{:>10}  {:<32} {}", format_size(a.size), a.name, a.bundle_id.as_deref().unwrap_or("-"));
    }
    println!("{:>10}  {} apps", format_size(apps.iter().map(|a| a.size).sum()), apps.len());
    Ok(())
}
