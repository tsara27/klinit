//! Rendering. Every clickable element registers itself in the `HitMap` as it is drawn.

mod apps;
mod dashboard;
mod dialog;
mod list;
mod widgets;

use ratatui::Frame;
use ratatui::layout::Constraint::Length;
use ratatui::layout::{Layout, Rect};
use ratatui::layout::Constraint::Min;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use super::app::App;
use super::hit::HitMap;
use super::model::{Action, Checklist, Screen};
use super::theme::Theme;
use crate::core::scanner::format_size;
use apps::draw_apps;
use dashboard::draw_dashboard;
use dialog::draw_dialog;
use list::list_screen;
use widgets::{muted, panel};

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

#[cfg(test)]
mod tests {
    use super::widgets::{fit, tail};
    use super::*;
    use crate::core::plan::{Category, Item, Plan};
    use crate::modules::apps::App as AppInfo;
    use crate::tui::model::{Dialog, Msg, ScanKind, ScanResult};
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
