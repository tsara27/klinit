//! Modal dialogs: confirmation, results, progress and help.

use ratatui::Frame;
use ratatui::layout::Constraint::{Length, Min};
use ratatui::layout::{Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph, Wrap};

use super::widgets::{SPINNER, buttons, normal, panel};
use crate::tui::hit::HitMap;
use crate::tui::model::{Action, Dialog};
use crate::tui::theme::Theme;

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

pub(super) fn draw_dialog(f: &mut Frame, dialog: &Dialog, theme: Theme, tick: usize, hits: &mut HitMap) {
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
