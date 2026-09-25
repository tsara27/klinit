//! Installed apps list with a details pane.

use ratatui::Frame;
use ratatui::layout::Constraint::{Length, Min, Percentage};
use ratatui::layout::{Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};

use super::widgets::{bar, buttons, draw_rows, fit, frac, muted, normal, panel, placeholder};
use crate::core::scanner::format_size;
use crate::tui::hit::HitMap;
use crate::tui::model::{Action, AppsView, Load};
use crate::tui::theme::Theme;

pub(super) fn draw_apps(f: &mut Frame, area: Rect, load: &mut Load<AppsView>, theme: Theme, tick: usize, hits: &mut HitMap) -> usize {
    let Load::Ready(v) = load else {
        placeholder(f, area, "Apps", load, theme, tick);
        return 0;
    };
    let [main, foot] = Layout::vertical([Min(3), Length(3)]).areas(area);
    let [list_a, detail_a] = Layout::horizontal([Percentage(55), Percentage(45)]).areas(main);
    let max = v.apps.iter().map(|a| a.size).max().unwrap_or(1);
    let apps = &v.apps;
    let sel = v.selected;
    let title = format!("Apps · {} · sorted by {}", apps.len(), if v.by_size { "size" } else { "name" });
    let rows = draw_rows(f, list_a, &title, sel, &mut v.offset, apps.len(), false, "No apps found", theme, hits, |i, w| {
        let a = &apps[i];
        let name_w = w.saturating_sub(9 + 1 + 8 + 1);
        let mut spans = vec![Span::raw(format!("{} ", fit(&a.name, name_w))), Span::raw(format!("{:>9} ", format_size(a.size)))];
        spans.extend(bar(8, frac(a.size, max), theme));
        Line::from(spans)
    });

    let block = panel("Details", theme);
    let inner = block.inner(detail_a);
    f.render_widget(block, detail_a);
    let selected = v.apps.get(v.selected);
    if let Some(a) = selected {
        let field = |k: &str, val: String| Line::from(vec![Span::styled(format!("{k:<9}"), muted()), Span::raw(val)]);
        let mut lines = vec![
            Line::styled(a.name.clone(), Style::new().add_modifier(Modifier::BOLD)),
            Line::raw(""),
            field("Version", a.version.clone().unwrap_or_else(|| "—".into())),
            field("Bundle", a.bundle_id.clone().unwrap_or_else(|| "—".into())),
            field("Size", format_size(a.size)),
            field("Path", a.path.display().to_string()),
            Line::raw(""),
        ];
        lines.push(if a.is_apple() {
            Line::styled("System app: never uninstalled.", Style::new().fg(Color::Yellow))
        } else {
            Line::styled("Uninstall removes the app and leftovers that match its bundle ID.", muted())
        });
        f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
    }

    let mut items = Vec::new();
    if selected.is_some_and(|a| !a.is_apple()) {
        let s = Style::new().fg(Color::Black).bg(theme.accent()).add_modifier(Modifier::BOLD);
        items.push(("Uninstall  (enter)".to_string(), Action::Primary, s));
    }
    items.push(("Sort (s)".into(), Action::Sort, normal(theme)));
    items.push(("Rescan (r)".into(), Action::Refresh, normal(theme)));
    buttons(f, foot, hits, &items);
    rows
}
