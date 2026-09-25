//! Interactive checklist for choosing which plan items to act on.

use anyhow::Result;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Modifier, Style};
use ratatui::widgets::{List, ListItem, ListState, Paragraph};

use crate::core::plan::Item;
use crate::core::scanner::format_size;

/// Returns the items the user ticked, or an empty list if they cancelled.
pub fn select(items: Vec<Item>, preselected: bool) -> Result<Vec<Item>> {
    if items.is_empty() {
        return Ok(items);
    }
    let mut checked = vec![preselected; items.len()];
    let mut state = ListState::default().with_selected(Some(0));
    let mut terminal = ratatui::init();
    let outcome = run(&mut terminal, &items, &mut checked, &mut state);
    ratatui::restore();
    Ok(if outcome? { items.into_iter().zip(checked).filter(|(_, c)| *c).map(|(i, _)| i).collect() } else { Vec::new() })
}

/// True on confirm, false on cancel.
fn run(terminal: &mut ratatui::DefaultTerminal, items: &[Item], checked: &mut [bool], state: &mut ListState) -> Result<bool> {
    loop {
        let total: u64 = items.iter().zip(checked.iter()).filter(|(_, c)| **c).map(|(i, _)| i.size).sum();
        let count = checked.iter().filter(|c| **c).count();
        terminal.draw(|f| {
            let [list_area, help_area] = Layout::vertical([Constraint::Min(1), Constraint::Length(2)]).areas(f.area());
            let rows: Vec<ListItem> = items
                .iter()
                .zip(checked.iter())
                .map(|(i, c)| {
                    ListItem::new(format!("[{}] {:>10}  [{}] {}", if *c { "x" } else { " " }, format_size(i.size), i.category, i.path.display()))
                })
                .collect();
            f.render_stateful_widget(List::new(rows).highlight_style(Style::new().add_modifier(Modifier::REVERSED)), list_area, state);
            f.render_widget(
                Paragraph::new(format!(
                    "{count} selected, {}\n↑/↓ move  space toggle  a all/none  enter confirm  q cancel",
                    format_size(total)
                )),
                help_area,
            );
        })?;
        let Event::Key(key) = event::read()? else { continue };
        if key.kind != KeyEventKind::Press {
            continue;
        }
        let cur = state.selected().unwrap_or(0);
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => state.select(Some(cur.saturating_sub(1))),
            KeyCode::Down | KeyCode::Char('j') => state.select(Some((cur + 1).min(items.len() - 1))),
            KeyCode::Char(' ') => checked[cur] = !checked[cur],
            KeyCode::Char('a') => {
                let all = checked.iter().all(|c| *c);
                checked.fill(!all);
            }
            KeyCode::Enter => return Ok(true),
            KeyCode::Char('q') | KeyCode::Esc => return Ok(false),
            _ => {}
        }
    }
}
