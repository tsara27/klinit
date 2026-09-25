//! Screens that show a tickable checklist: cleanup, leftovers, large files, duplicates.

use ratatui::Frame;
use ratatui::layout::Constraint::{Length, Min};
use ratatui::layout::{Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

use super::widgets::{bar, buttons, draw_rows, frac, muted, normal, placeholder, fit, tail};
use crate::core::scanner::format_size;
use crate::tui::hit::HitMap;
use crate::tui::model::{Action, Checklist, Load};
use crate::tui::theme::Theme;

fn checklist_line(list: &Checklist, i: usize, w: usize, max: u64, theme: Theme) -> Line<'static> {
    let item = &list.items[i];
    let mark = if list.checked[i] { Span::styled("[x] ", Style::new().fg(Color::Green)) } else { Span::styled("[ ] ", muted()) };
    let size = Span::raw(format!("{:>9} ", format_size(item.size)));
    let path = item.path.display().to_string();
    let mut spans = vec![mark, size];
    if w >= 56 {
        spans.extend(bar(8, frac(item.size, max), theme));
        spans.push(Span::styled(format!(" {} ", fit(item.category.as_str(), 9)), muted()));
        spans.push(Span::raw(tail(&path, w.saturating_sub(33))));
    } else {
        spans.push(Span::raw(tail(&path, w.saturating_sub(14))));
    }
    Line::from(spans)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn list_screen<T>(
    f: &mut Frame,
    area: Rect,
    title: &str,
    verb: &str,
    load: &mut Load<T>,
    get: impl FnOnce(&mut T) -> &mut Checklist,
    permanent: bool,
    theme: Theme,
    tick: usize,
    hits: &mut HitMap,
) -> usize {
    let Load::Ready(v) = load else {
        placeholder(f, area, title, load, theme, tick);
        return 0;
    };
    let list = get(v);
    let [main, foot] = Layout::vertical([Min(3), Length(3)]).areas(area);
    let (n, size) = list.stats();
    let max = list.items.iter().map(|i| i.size).max().unwrap_or(1);
    let total = list.items.iter().map(|i| i.size).sum::<u64>();
    let mut heading = format!("{title} · {} items · {}", list.items.len(), format_size(total));
    if !list.warnings.is_empty() {
        heading.push_str(&format!(" · {} warning(s)", list.warnings.len()));
    }
    let list_ref = &*list;
    let (sel, mut off) = (list_ref.selected, list_ref.offset);
    let rows = draw_rows(f, main, &heading, sel, &mut off, list_ref.items.len(), true, "Nothing found ✓", theme, hits, |i, w| {
        checklist_line(list_ref, i, w, max, theme)
    });
    list.offset = off;

    let verb = if permanent && verb != "Clean" { "Delete" } else { verb };
    let primary = if n > 0 {
        Style::new().fg(Color::Black).bg(if permanent { Color::Red } else { theme.accent() }).add_modifier(Modifier::BOLD)
    } else {
        muted()
    };
    buttons(
        f,
        foot,
        hits,
        &[
            (format!("{verb} {n} · {}  (enter)", format_size(size)), Action::Primary, primary),
            ("Select all/none (a)".into(), Action::SelectAll, normal(theme)),
            ("Rescan (r)".into(), Action::Refresh, normal(theme)),
        ],
    );
    rows
}
