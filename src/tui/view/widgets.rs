//! Drawing helpers shared by every screen.

use ratatui::Frame;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Paragraph};

use crate::tui::hit::HitMap;
use crate::tui::model::{Action, Load};
use crate::tui::theme::Theme;

pub(super) const DIM: Color = Color::DarkGray;
pub(super) const SPINNER: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

pub(super) fn panel(title: &str, theme: Theme) -> Block<'static> {
    Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(theme.border()))
        .title(Span::styled(format!(" {title} "), Style::new().fg(theme.accent()).add_modifier(Modifier::BOLD)))
}

/// Pads or truncates (with an ellipsis) to exactly `w` columns.
pub(super) fn fit(s: &str, w: usize) -> String {
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
pub(super) fn tail(s: &str, w: usize) -> String {
    let n = s.chars().count();
    if n <= w {
        s.to_string()
    } else {
        let t: String = s.chars().skip(n - w.saturating_sub(1)).collect();
        format!("…{t}")
    }
}

pub(super) fn bar(width: usize, frac: f64, theme: Theme) -> Vec<Span<'static>> {
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

pub(super) fn frac(size: u64, max: u64) -> f64 {
    size as f64 / max.max(1) as f64
}

pub(super) fn buttons(f: &mut Frame, area: Rect, hits: &mut HitMap, items: &[(String, Action, Style)]) {
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

pub(super) fn normal(theme: Theme) -> Style {
    Style::new().fg(theme.accent())
}

pub(super) fn muted() -> Style {
    Style::new().fg(DIM)
}

pub(super) fn placeholder<T>(f: &mut Frame, area: Rect, title: &str, load: &Load<T>, theme: Theme, tick: usize) {
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
pub(super) fn draw_rows(
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
