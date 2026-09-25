//! Dashboard: disk usage and reclaimable space per category.

use ratatui::Frame;
use ratatui::layout::Constraint::{Length, Min, Percentage};
use ratatui::layout::{Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use super::widgets::{SPINNER, bar, buttons, fit, frac, muted, normal, panel, placeholder};
use crate::core::scanner::format_size;
use crate::tui::hit::HitMap;
use crate::tui::model::{Action, Load, Reclaim, Screen};
use crate::tui::theme::Theme;

pub(super) fn draw_dashboard(f: &mut Frame, area: Rect, cleanup: &Load<Reclaim>, disk: Option<(u64, u64)>, theme: Theme, tick: usize, hits: &mut HitMap) {
    let [left, right] = Layout::horizontal([Percentage(40), Percentage(60)]).areas(area);
    let [disk_a, actions_a, total_a] = Layout::vertical([Length(4), Length(3), Min(0)]).areas(left);

    let block = panel("Disk", theme);
    let inner = block.inner(disk_a);
    f.render_widget(block, disk_a);
    let lines = match disk {
        Some((used, total)) => {
            let fr = frac(used, total);
            let mut spans = bar((inner.width as usize).saturating_sub(6), fr, theme);
            spans.push(Span::raw(format!(" {:>3.0}%", fr * 100.0)));
            vec![Line::from(spans), Line::styled(format!("{} used of {}", format_size(used), format_size(total)), muted())]
        }
        None => vec![Line::styled("unavailable", muted())],
    };
    f.render_widget(Paragraph::new(lines), inner);

    buttons(
        f,
        actions_a,
        hits,
        &[("Review cleanup".into(), Action::Tab(Screen::Cleanup), normal(theme)), ("Rescan (r)".into(), Action::Refresh, normal(theme))],
    );

    let block = panel("Reclaimable", theme);
    let inner = block.inner(total_a);
    f.render_widget(block, total_a);
    let Load::Ready(r) = cleanup else {
        placeholder(f, right, "Categories", cleanup, theme, tick);
        let text = if matches!(cleanup, Load::Loading) { format!("{} Scanning…", SPINNER[tick % SPINNER.len()]) } else { String::new() };
        f.render_widget(Paragraph::new(text).style(muted()), inner);
        return;
    };
    let total: u64 = r.list.items.iter().map(|i| i.size).sum();
    f.render_widget(
        Paragraph::new(vec![
            Line::styled(format_size(total), Style::new().fg(theme.accent()).add_modifier(Modifier::BOLD)),
            Line::styled(format!("in {} items (trash excluded)", r.list.items.len()), muted()),
            Line::styled(format!("{} warning(s)", r.list.warnings.len()), muted()),
        ]),
        inner,
    );

    let block = panel("Categories", theme);
    let inner = block.inner(right);
    f.render_widget(block, right);
    let max = r.rows.iter().map(|c| c.size).max().unwrap_or(1);
    let bar_w = (inner.width as usize).saturating_sub(11 + 10 + 7);
    for (n, c) in r.rows.iter().enumerate().take(inner.height as usize) {
        let mut spans = vec![Span::raw(format!("{} ", fit(c.id.as_str(), 10)))];
        spans.extend(bar(bar_w, frac(c.size, max), theme));
        spans.push(Span::raw(format!(" {:>9}", format_size(c.size))));
        spans.push(Span::styled(format!(" {:>4}", c.count), muted()));
        let row = Rect::new(inner.x, inner.y + n as u16, inner.width, 1);
        f.render_widget(Paragraph::new(Line::from(spans)), row);
        hits.add(row, Action::Tab(Screen::Cleanup));
    }
}
