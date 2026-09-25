use crate::core::executor::{Mode, Report};
use crate::core::plan::{Category, Plan};
use crate::core::scanner::format_size;
use crate::modules::apps::App;
use crate::modules::large::{DupeGroup, FileInfo};

pub fn print_scan(rows: &[(Category, &str, Plan)], json: bool) -> anyhow::Result<()> {
    if json {
        let out: Vec<_> = rows
            .iter()
            .map(|(id, desc, p)| serde_json::json!({"category": id.as_str(), "description": desc, "items": p.items.len(), "size": p.total_size(), "warnings": p.warnings}))
            .collect();
        println!("{}", serde_json::to_string_pretty(&out)?);
        return Ok(());
    }
    println!("{:<10} {:>6} {:>10}  DESCRIPTION", "CATEGORY", "ITEMS", "SIZE");
    for (id, desc, p) in rows {
        println!("{:<10} {:>6} {:>10}  {}", id, p.items.len(), format_size(p.total_size()), desc);
    }
    let total: u64 = rows.iter().filter(|(id, ..)| !id.is_trash()).map(|(.., p)| p.total_size()).sum();
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
    items.sort_by_key(|i| std::cmp::Reverse(i.size));
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
    if let Some(e) = &report.log_error {
        eprintln!("warning: could not write the action log: {e}");
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
    println!("{:>10}  {:<32} BUNDLE ID", "SIZE", "NAME");
    for a in apps {
        println!("{:>10}  {:<32} {}", format_size(a.size), a.name, a.bundle_id.as_deref().unwrap_or("-"));
    }
    println!("{:>10}  {} apps", format_size(apps.iter().map(|a| a.size).sum()), apps.len());
    Ok(())
}

pub fn print_plan(plan: &Plan, json: bool) -> anyhow::Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(plan)?);
        return Ok(());
    }
    for i in &plan.items {
        println!("{:>10}  {}", format_size(i.size), i.path.display());
    }
    println!("{:>10}  {} items", format_size(plan.total_size()), plan.items.len());
    Ok(())
}

pub fn print_files(files: &[FileInfo], json: bool) -> anyhow::Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(files)?);
        return Ok(());
    }
    for f in files {
        println!("{:>10}  {:>5}d  {}", format_size(f.size), f.age_days, f.path.display());
    }
    Ok(())
}

pub fn print_dupes(groups: &[DupeGroup], json: bool) -> anyhow::Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(groups)?);
        return Ok(());
    }
    for g in groups {
        println!("{} wasted ({} copies of {})", format_size(g.wasted()), g.files.len(), format_size(g.size));
        for (i, f) in g.files.iter().enumerate() {
            println!("  {} {}", if i == 0 { "keep  " } else { "extra " }, f.path.display());
        }
    }
    println!("{} reclaimable in {} groups", format_size(groups.iter().map(|g| g.wasted()).sum()), groups.len());
    Ok(())
}
