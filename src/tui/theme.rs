//! Colors. Truecolor gradients when the terminal supports them, plain ANSI otherwise.

use ratatui::style::{Color, Style};

#[derive(Clone, Copy, Debug)]
pub struct Theme {
    pub truecolor: bool,
}

impl Theme {
    pub fn detect() -> Self {
        let ct = std::env::var("COLORTERM").unwrap_or_default();
        Self { truecolor: ct == "truecolor" || ct == "24bit" }
    }

    pub fn accent(&self) -> Color {
        if self.truecolor { Color::Rgb(90, 200, 250) } else { Color::Cyan }
    }

    pub fn border(&self) -> Color {
        if self.truecolor { Color::Rgb(110, 120, 150) } else { Color::DarkGray }
    }

    pub fn sel(&self) -> Style {
        Style::new().bg(Color::Indexed(238))
    }

    /// Green (0.0) through yellow to red (1.0).
    pub fn gradient(&self, t: f64) -> Color {
        let t = t.clamp(0.0, 1.0);
        if !self.truecolor {
            return if t < 0.4 { Color::Green } else if t < 0.75 { Color::Yellow } else { Color::Red };
        }
        let ((r1, g1, b1), (r2, g2, b2), u) = if t < 0.5 {
            ((80.0, 200.0, 120.0), (240.0, 200.0, 60.0), t * 2.0)
        } else {
            ((240.0, 200.0, 60.0), (230.0, 80.0, 80.0), (t - 0.5) * 2.0)
        };
        let l = |a: f64, b: f64| (a + (b - a) * u) as u8;
        Color::Rgb(l(r1, r2), l(g1, g2), l(b1, b2))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gradient_ends_are_green_and_red() {
        let t = Theme { truecolor: true };
        assert_eq!(t.gradient(0.0), Color::Rgb(80, 200, 120));
        assert_eq!(t.gradient(1.0), Color::Rgb(230, 80, 80));
        assert_eq!(Theme { truecolor: false }.gradient(0.9), Color::Red);
    }
}
