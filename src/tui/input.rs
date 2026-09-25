//! Key press to `Action` mapping.

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::app::App;
use super::model::{Action, Screen};

impl App {
    pub(super) fn key_action(&self, key: KeyEvent) -> Option<Action> {
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            return matches!(key.code, KeyCode::Char('c')).then_some(Action::Quit);
        }
        if self.dialog.is_some() {
            return match key.code {
                KeyCode::Enter | KeyCode::Char('y') => Some(Action::Confirm),
                KeyCode::Esc | KeyCode::Char('n' | 'q') => Some(Action::Cancel),
                _ => None,
            };
        }
        let page = self.view_rows.max(1) as isize;
        Some(match key.code {
            KeyCode::Char('q') | KeyCode::Esc => Action::Quit,
            KeyCode::Tab => Action::NextTab,
            KeyCode::BackTab => Action::PrevTab,
            KeyCode::Char(c @ '1'..='6') => Action::Tab(Screen::ALL[c as usize - '1' as usize]),
            KeyCode::Up | KeyCode::Char('k') => Action::Move(-1),
            KeyCode::Down | KeyCode::Char('j') => Action::Move(1),
            KeyCode::PageUp => Action::Move(-page),
            KeyCode::PageDown => Action::Move(page),
            KeyCode::Home => Action::Move(isize::MIN / 2),
            KeyCode::End => Action::Move(isize::MAX / 2),
            KeyCode::Char(' ') => Action::ToggleCurrent,
            KeyCode::Char('a') => Action::SelectAll,
            KeyCode::Char('r') => Action::Refresh,
            KeyCode::Char('s') => Action::Sort,
            KeyCode::Char('d') => Action::TogglePermanent,
            KeyCode::Char('?') => Action::Help,
            KeyCode::Enter => Action::Primary,
            _ => return None,
        })
    }
}
