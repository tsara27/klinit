//! Click targets. Every draw registers `(Rect, Action)` pairs; a mouse click is resolved against them.

use ratatui::layout::{Position, Rect};

use super::app::Action;

#[derive(Default)]
pub struct HitMap(Vec<(Rect, Action)>);

impl HitMap {
    pub fn clear(&mut self) {
        self.0.clear();
    }

    pub fn add(&mut self, rect: Rect, action: Action) {
        self.0.push((rect, action));
    }

    /// The most recently registered (topmost) target under the point.
    pub fn at(&self, x: u16, y: u16) -> Option<Action> {
        self.0.iter().rev().find(|(r, _)| r.contains(Position::new(x, y))).map(|(_, a)| *a)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn topmost_target_wins() {
        let mut h = HitMap::default();
        h.add(Rect::new(0, 0, 10, 10), Action::Row(1));
        h.add(Rect::new(0, 0, 3, 1), Action::Toggle(1));
        assert_eq!(h.at(1, 0), Some(Action::Toggle(1)));
        assert_eq!(h.at(5, 0), Some(Action::Row(1)));
        assert_eq!(h.at(20, 20), None);
        h.clear();
        assert_eq!(h.at(1, 0), None);
    }
}
