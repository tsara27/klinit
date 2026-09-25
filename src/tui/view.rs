//! Rendering. Every clickable element registers itself in the `HitMap` as it is drawn.

use ratatui::Frame;
use ratatui::layout::Constraint::{Length, Min, Percentage};
use ratatui::layout::{Alignment, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Clear, Paragraph, Wrap};

use super::app::{Action, App, AppsView, Checklist, Dialog, Load, Reclaim, Screen};
use super::hit::HitMap;
use super::theme::Theme;
use crate::core::scanner::format_size;

const DIM: Color = Color::DarkGray;
const SPINNER: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

pub fn draw(f: &mut Frame, app: &mut App) {
    let theme = app.theme;
    let tick = app.tick;
    app.hits.clear();
    let [head, body, foot] = Layout::vertical([Length(3), Min(1), Length(1)]).areas(f.area());
    draw_tabs(f, head, app.screen, theme, &mut app.hits);

    let sel = app.checklist().map(Checklist::stats);
    let permanent = app.permanent;
    let hits = &mut app.hits;
    app.view_rows = match app.screen {
        Screen::Dashboard => {
            draw_dashboard(f, body, &app.cleanup, app.disk, theme, tick, hits);
            app.view_rows
        }
        Screen::Apps => draw_apps(f, body, &mut app.apps, theme, tick, hits),
        Screen::Cleanup => {
            list_screen(f, body, "Cleanup", "Clean", &mut app.cleanup, |r| &mut r.list, permanent, theme, tick, hits)
        }
        Screen::Leftovers => {
            list_screen(f, body, "Leftovers", "Remove", &mut app.leftovers, |l| l, permanent, theme, tick, hits)
        }
        Screen::Large => list_screen(f, body, "Large files", "Trash", &mut app.large, |l| l, permanent, theme, tick, hits),
        Screen::Dupes => list_screen(f, body, "Duplicates", "Remove extras", &mut app.dupes, |l| l, permanent, theme, tick, hits),
    }
    .max(1);

    draw_status(f, foot, sel, permanent, hits);
    if let Some(d) = &app.dialog {
        draw_dialog(f, d, theme, tick, hits);
    }
}

fn panel(title: &str, theme: Theme) -> Block<'static> {
    Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(theme.border()))
        .title(Span::styled(format!(" {title} "), Style::new().fg(theme.accent()).add_modifier(Modifier::BOLD)))
}

/// Pads or truncates (with an ellipsis) to exactly `w` columns.
fn fit(s: &str, w: usize) -> String {
    let n = s.chars().count();
    if n > w {
        let mut t: String = s.chars().take(w.saturating_sub(1)).collect();
        if w > 0 {
            t.push('…');
        }
        t
    } else {
        format!("{s}{}", " ".repeat(w - n))
    }
}

/// Keeps the end of the string, which is the informative part of a path.
fn tail(s: &str, w: usize) -> String {
    let n = s.chars().count();
    if n <= w {
        s.to_string()
    } else {
        let t: String = s.chars().skip(n - w.saturating_sub(1)).collect();
        format!("…{t}")
    }
}

fn bar(width: usize, frac: f64, theme: Theme) -> Vec<Span<'static>> {
    let filled = ((frac.clamp(0.0, 1.0) * width as f64).round() as usize).min(width);
    (0..width)
        .map(|i| {
            if i < filled {
                Span::styled("█", Style::new().fg(theme.gradient(i as f64 / width.max(1) as f64)))
            } else {
                Span::styled("░", Style::new().fg(DIM))
            }
        })
        .collect()
}

fn frac(size: u64, max: u64) -> f64 {
    size as f64 / max.max(1) as f64
}

fn buttons(f: &mut Frame, area: Rect, hits: &mut HitMap, items: &[(String, Action, Style)]) {
    let mut x = area.x;
    for (label, action, style) in items {
        let w = label.chars().count() as u16 + 4;
        if x + w > area.right() {
            break;
        }
        let r = Rect::new(x, area.y, w, area.height);
        let block = Block::bordered().border_type(BorderType::Rounded).border_style(*style);
        f.render_widget(Paragraph::new(label.as_str()).alignment(Alignment::Center).style(*style).block(block), r);
        hits.add(r, *action);
        x += w + 1;
    }
}

fn normal(theme: Theme) -> Style {
    Style::new().fg(theme.accent())
}

fn muted() -> Style {
    Style::new().fg(DIM)
}

fn draw_tabs(f: &mut Frame, area: Rect, current: Screen, theme: Theme, hits: &mut HitMap) {
    let block = panel("klinit", theme);
    let inner = block.inner(area);
    f.render_widget(block, area);
    let mut x = inner.x;
    let mut spans = Vec::new();
    for (i, s) in Screen::ALL.iter().enumerate() {
        let label = format!(" {} {} ", i + 1, s.title());
        let w = label.chars().count() as u16;
        let style = if *s == current {
            Style::new().fg(Color::Black).bg(theme.accent()).add_modifier(Modifier::BOLD)
        } else {
            Style::new().fg(Color::Gray)
        };
        if x + w <= inner.right() {
            hits.add(Rect::new(x, inner.y, w, 1), Action::Tab(*s));
        }
        spans.push(Span::styled(label, style));
        spans.push(Span::raw(" "));
        x += w + 1;
    }
    f.render_widget(Paragraph::new(Line::from(spans)), inner);
}

fn draw_status(f: &mut Frame, area: Rect, sel: Option<(usize, u64)>, permanent: bool, hits: &mut HitMap) {
    let (badge, style) = if permanent {
        (" PERMANENT delete ", Style::new().fg(Color::White).bg(Color::Red).add_modifier(Modifier::BOLD))
    } else {
        (" Trash ", Style::new().fg(Color::Black).bg(Color::Green))
    };
    let bw = badge.chars().count() as u16;
    let badge_area = Rect::new(area.right().saturating_sub(bw).max(area.x), area.y, bw.min(area.width), 1);
    f.render_widget(Paragraph::new(badge).style(style), badge_area);
    hits.add(badge_area, Action::TogglePermanent);

    let hint = "space toggle · a all · enter act · tab next · r rescan · d mode · ? help · q quit";
    let text = match sel {
        Some((n, s)) => format!(" {n} selected, {} · {hint}", format_size(s)),
        None => format!(" {hint}"),
    };
    let left = Rect::new(area.x, area.y, area.width.saturating_sub(bw + 1), 1);
    f.render_widget(Paragraph::new(text).style(muted()), left);
}

fn placeholder<T>(f: &mut Frame, area: Rect, title: &str, load: &Load<T>, theme: Theme, tick: usize) {
    let block = panel(title, theme);
    let inner = block.inner(area);
    f.render_widget(block, area);
    let text = match load {
        Load::Idle => "Press r to scan".to_string(),
        Load::Loading => format!("{} Scanning…", SPINNER[tick % SPINNER.len()]),
        Load::Ready(_) => String::new(),
    };
    let y = inner.y + inner.height / 2;
    f.render_widget(Paragraph::new(text).alignment(Alignment::Center).style(muted()), Rect::new(inner.x, y, inner.width, 1.min(inner.height)));
}

/// A scrolling, selectable row list. Returns how many rows fit.
#[allow(clippy::too_many_arguments)]
fn draw_rows(
    f: &mut Frame,
    area: Rect,
    title: &str,
    selected: usize,
    offset: &mut usize,
    count: usize,
    checkbox: bool,
    empty: &str,
    theme: Theme,
    hits: &mut HitMap,
    line: impl Fn(usize, usize) -> Line<'static>,
) -> usize {
    let block = panel(title, theme);
    let inner = block.inner(area);
    f.render_widget(block, area);
    let rows = inner.height as usize;
    if count == 0 {
        f.render_widget(Paragraph::new(empty).alignment(Alignment::Center).style(muted()), Rect::new(inner.x, inner.y + inner.height / 2, inner.width, 1.min(inner.height)));
        return rows;
    }
    if selected < *offset {
        *offset = selected;
    } else if rows > 0 && selected >= *offset + rows {
        *offset = selected + 1 - rows;
    }
    *offset = (*offset).min(count.saturating_sub(rows));
    for (n, i) in (*offset..count.min(*offset + rows)).enumerate() {
        let r = Rect::new(inner.x, inner.y + n as u16, inner.width, 1);
        let mut p = Paragraph::new(line(i, inner.width as usize));
        if i == selected {
            p = p.style(theme.sel());
        }
        f.render_widget(p, r);
        hits.add(r, Action::Row(i));
        if checkbox {
            hits.add(Rect::new(r.x, r.y, 3.min(r.width), 1), Action::Toggle(i));
        }
    }
    rows
}

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
fn list_screen<T>(
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

fn draw_dashboard(f: &mut Frame, area: Rect, cleanup: &Load<Reclaim>, disk: Option<(u64, u64)>, theme: Theme, tick: usize, hits: &mut HitMap) {
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

fn draw_apps(f: &mut Frame, area: Rect, load: &mut Load<AppsView>, theme: Theme, tick: usize, hits: &mut HitMap) -> usize {
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

const HELP: &[(&str, &str)] = &[
    ("tab / 1-6", "switch screen (or click a tab)"),
    ("↑ ↓ j k", "move; mouse wheel scrolls"),
    ("space", "tick / untick (or click the box)"),
    ("a", "select all / none"),
    ("enter", "clean, remove or uninstall selected"),
    ("r", "rescan this screen"),
    ("s", "sort apps by name / size"),
    ("d", "toggle Trash / permanent delete"),
    ("q", "quit"),
];

fn draw_dialog(f: &mut Frame, dialog: &Dialog, theme: Theme, tick: usize, hits: &mut HitMap) {
    hits.clear();
    let ok = Style::new().fg(Color::Black).bg(theme.accent()).add_modifier(Modifier::BOLD);
    let (title, lines, btns): (String, Vec<Line>, Vec<(String, Action, Style)>) = match dialog {
        Dialog::Confirm { title, lines, mode, stage, .. } => {
            let danger = *mode == crate::core::executor::Mode::Permanent;
            let lines = lines
                .iter()
                .map(|l| {
                    if l.starts_with("PERMANENT") {
                        Line::styled(l.clone(), Style::new().fg(Color::Red).add_modifier(Modifier::BOLD))
                    } else {
                        Line::raw(l.clone())
                    }
                })
                .collect();
            let label = match (danger, *stage) {
                (true, 0) => "Continue  (enter)",
                (true, _) => "Delete forever  (enter)",
                _ => "Confirm  (enter)",
            };
            let go = if danger { Style::new().fg(Color::White).bg(Color::Red).add_modifier(Modifier::BOLD) } else { ok };
            (title.clone(), lines, vec![(label.into(), Action::Confirm, go), ("Cancel (esc)".into(), Action::Cancel, normal(theme))])
        }
        Dialog::Result { title, lines } => {
            (title.clone(), lines.iter().cloned().map(Line::raw).collect(), vec![("OK  (enter)".into(), Action::Cancel, ok)])
        }
        Dialog::Busy(msg) => ("Working".into(), vec![Line::raw(format!("{} {msg}", SPINNER[tick % SPINNER.len()]))], vec![]),
        Dialog::Help => (
            "Help".into(),
            HELP.iter().map(|(k, d)| Line::from(vec![Span::styled(format!("{k:<11}"), Style::new().fg(theme.accent())), Span::raw(*d)])).collect(),
            vec![("Close  (esc)".into(), Action::Cancel, ok)],
        ),
    };
    let area = f.area();
    let w = area.width.saturating_sub(4).min(76);
    let h = (lines.len() as u16 + 2 + if btns.is_empty() { 0 } else { 3 } + 1).min(area.height.saturating_sub(2));
    let r = Rect::new(area.x + (area.width - w) / 2, area.y + (area.height.saturating_sub(h)) / 2, w, h);
    f.render_widget(Clear, r);
    let block = panel(&title, theme);
    let inner = block.inner(r);
    f.render_widget(block, r);
    let foot = if btns.is_empty() { 0 } else { 3.min(inner.height) };
    let [text, btn_a] = Layout::vertical([Min(0), Length(foot)]).areas(inner);
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), text);
    buttons(f, btn_a, hits, &btns);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::plan::{Category, Item, Plan};
    use crate::modules::apps::App as AppInfo;
    use crate::tui::app::{Msg, ScanKind, ScanResult};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

    fn plan(n: usize) -> Plan {
        Plan {
            items: (0..n)
                .map(|i| Item { path: format!("/Users/x/Library/Caches/some.long.bundle.id.{i}").into(), size: 1000 * (i as u64 + 1), category: Category::Caches, reason: "r".into() })
                .collect(),
            warnings: vec!["needs Full Disk Access".into()],
        }
    }

    fn loaded() -> App {
        let mut app = App::new(Theme { truecolor: true });
        app.update(Msg::Scanned(ScanResult::Cleanup(vec![(Category::Caches, "caches", plan(30)), (Category::Trash, "trash", plan(2))])));
        app.update(Msg::Scanned(ScanResult::Apps(vec![AppInfo {
            path: "/Applications/Foo.app".into(),
            name: "Foo".into(),
            bundle_id: Some("com.foo".into()),
            version: Some("1.0".into()),
            size: 5_000_000,
        }])));
        for k in [ScanKind::Leftovers, ScanKind::Large, ScanKind::Dupes] {
            app.update(Msg::Scanned(ScanResult::List(k, plan(3))));
        }
        app.update(Msg::Disk(Some((180_000_000_000, 250_000_000_000))));
        app
    }

    fn render(app: &mut App, w: u16, h: u16) -> String {
        let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
        t.draw(|f| draw(f, app)).unwrap();
        t.backend().buffer().content().iter().map(|c| c.symbol()).collect()
    }

    #[test]
    fn every_screen_renders_at_many_sizes_without_panicking() {
        let mut app = loaded();
        for screen in Screen::ALL {
            app.screen = screen;
            for (w, h) in [(120, 40), (60, 20), (20, 6), (5, 2), (0, 0)] {
                render(&mut app, w, h);
            }
        }
    }

    #[test]
    fn unloaded_screens_and_dialogs_render() {
        let mut app = App::new(Theme { truecolor: false });
        for screen in Screen::ALL {
            app.screen = screen;
            render(&mut app, 80, 24);
        }
        for d in [Dialog::Help, Dialog::Busy("x".into()), Dialog::Result { title: "t".into(), lines: vec!["l".into()] }] {
            app.dialog = Some(d);
            render(&mut app, 80, 24);
            render(&mut app, 12, 4);
        }
    }

    #[test]
    fn dashboard_shows_categories_and_disk() {
        let mut app = loaded();
        let text = render(&mut app, 120, 30);
        assert!(text.contains("caches") && text.contains("Disk") && text.contains("72%"));
    }

    fn click(app: &mut App, x: u16, y: u16) {
        app.update(Msg::Mouse(MouseEvent { kind: MouseEventKind::Down(MouseButton::Left), column: x, row: y, modifiers: KeyModifiers::NONE }));
    }

    #[test]
    fn clicking_a_tab_switches_screen() {
        let mut app = loaded();
        render(&mut app, 120, 30);
        // Tabs start at x=1 inside the border: " 1 Dashboard " is 13 wide plus a space.
        click(&mut app, 16, 1);
        assert_eq!(app.screen, Screen::Apps);
    }

    #[test]
    fn clicking_a_checkbox_toggles_and_clicking_a_row_selects() {
        let mut app = loaded();
        app.screen = Screen::Cleanup;
        render(&mut app, 120, 30);
        let before = app.checklist().unwrap().stats().0;
        click(&mut app, 2, 4); // third row's checkbox: rows start at y=4
        assert_eq!(app.checklist().unwrap().stats().0, before - 1);
        click(&mut app, 60, 6);
        assert_eq!(app.checklist().unwrap().selected, 2);
    }

    #[test]
    fn dialog_swallows_clicks_meant_for_the_screen_behind_it() {
        let mut app = loaded();
        app.screen = Screen::Leftovers;
        app.dialog = Some(Dialog::Help);
        render(&mut app, 120, 30);
        click(&mut app, 16, 1); // where the Apps tab is
        assert_eq!(app.screen, Screen::Leftovers);
    }

    #[test]
    fn fit_and_tail_handle_edges() {
        assert_eq!(fit("abc", 5), "abc  ");
        assert_eq!(fit("abcdef", 4), "abc…");
        assert_eq!(fit("abc", 0), "");
        assert_eq!(tail("/a/b/c/d", 4), "…c/d");
        assert_eq!(tail("ab", 0), "…");
    }
}
