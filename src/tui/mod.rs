//! Full-screen, mouse-aware interface (`klinit` with no subcommand) and the plan checklist
//! used by `--interactive`.

mod app;
mod checklist;
mod hit;
mod theme;
mod view;
mod worker;

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc;
use std::time::Duration;

use anyhow::Result;
use ratatui::crossterm::event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyEventKind, MouseEventKind};

use crate::core::safety::Guard;
use app::{App, Msg};

pub use checklist::select;

/// What background workers need; cheap to clone.
#[derive(Clone)]
pub struct Ctx {
    pub home: PathBuf,
    pub app_dirs: Vec<PathBuf>,
    /// Guard for cleanups.
    pub guard: Arc<Guard>,
    /// Same, but also allows removing app bundles from the application folders.
    pub app_guard: Arc<Guard>,
}

pub fn run(ctx: Ctx) -> Result<()> {
    let mut terminal = ratatui::init();
    let mouse = std::env::var_os("KLINIT_NO_MOUSE").is_none();
    if mouse {
        let _ = ratatui::crossterm::execute!(std::io::stdout(), EnableMouseCapture);
        // ratatui's hook restores the screen; also release the mouse so a panic leaves a sane shell.
        let prev = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let _ = ratatui::crossterm::execute!(std::io::stdout(), DisableMouseCapture);
            prev(info);
        }));
    }
    let result = event_loop(&mut terminal, &ctx);
    if mouse {
        let _ = ratatui::crossterm::execute!(std::io::stdout(), DisableMouseCapture);
    }
    ratatui::restore();
    result
}

fn event_loop(terminal: &mut ratatui::DefaultTerminal, ctx: &Ctx) -> Result<()> {
    let (tx, rx) = mpsc::channel::<Msg>();
    let mut app = App::new(theme::Theme::detect());
    let mut effects = app.start();
    loop {
        for e in effects.drain(..) {
            worker::spawn(e, ctx, &tx);
        }
        terminal.draw(|f| view::draw(f, &mut app))?;
        if app.quit {
            return Ok(());
        }
        let mut msgs = Vec::new();
        if event::poll(Duration::from_millis(100))? {
            loop {
                match event::read()? {
                    Event::Key(k) if k.kind == KeyEventKind::Press => msgs.push(Msg::Key(k)),
                    Event::Mouse(m) if !matches!(m.kind, MouseEventKind::Moved | MouseEventKind::Drag(_)) => msgs.push(Msg::Mouse(m)),
                    _ => {}
                }
                if !event::poll(Duration::ZERO)? {
                    break;
                }
            }
        }
        msgs.extend(rx.try_iter());
        msgs.push(Msg::Tick);
        for m in msgs {
            effects.extend(app.update(m));
        }
    }
}
