//! Data types shared by the state machine and the renderer.

use ratatui::crossterm::event::{KeyEvent, MouseEvent};

use crate::core::executor::{Mode, Report};
use crate::core::plan::{Category, Item, Plan};
use crate::modules::apps::App as AppInfo;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Screen {
    Dashboard,
    Apps,
    Cleanup,
    Leftovers,
    Large,
    Dupes,
}

impl Screen {
    pub const ALL: [Screen; 6] =
        [Screen::Dashboard, Screen::Apps, Screen::Cleanup, Screen::Leftovers, Screen::Large, Screen::Dupes];

    pub fn title(self) -> &'static str {
        match self {
            Screen::Dashboard => "Dashboard",
            Screen::Apps => "Apps",
            Screen::Cleanup => "Cleanup",
            Screen::Leftovers => "Leftovers",
            Screen::Large => "Large files",
            Screen::Dupes => "Duplicates",
        }
    }

    pub fn kind(self) -> ScanKind {
        match self {
            Screen::Dashboard | Screen::Cleanup => ScanKind::Cleanup,
            Screen::Apps => ScanKind::Apps,
            Screen::Leftovers => ScanKind::Leftovers,
            Screen::Large => ScanKind::Large,
            Screen::Dupes => ScanKind::Dupes,
        }
    }

    pub fn index(self) -> usize {
        Self::ALL.iter().position(|s| *s == self).unwrap_or(0)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ScanKind {
    Cleanup,
    Apps,
    Leftovers,
    Large,
    Dupes,
}

/// Something a key press or click asks for.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Action {
    Quit,
    Tab(Screen),
    NextTab,
    PrevTab,
    Move(isize),
    /// Select the row at this index.
    Row(usize),
    /// Select and tick/untick the row at this index.
    Toggle(usize),
    ToggleCurrent,
    SelectAll,
    Refresh,
    Sort,
    /// The screen's main button: clean, remove or uninstall.
    Primary,
    TogglePermanent,
    Help,
    Confirm,
    Cancel,
}

/// Work for the runner; keeps `act` free of I/O.
#[derive(Debug)]
pub enum Effect {
    Scan(ScanKind),
    PlanUninstall(AppInfo),
    Execute { plan: Plan, mode: Mode, apps: bool },
}

pub struct CatRow {
    pub id: Category,
    pub size: u64,
    pub count: usize,
}

pub enum ScanResult {
    Cleanup(Vec<(Category, &'static str, Plan)>),
    Apps(Vec<AppInfo>),
    List(ScanKind, Plan),
}

pub enum Msg {
    Key(KeyEvent),
    Mouse(MouseEvent),
    Scanned(ScanResult),
    Disk(Option<(u64, u64)>),
    Planned(Result<Plan, String>),
    Progress { done: usize, total: usize, name: String },
    Executed(Report, Mode),
    Tick,
}

pub enum Load<T> {
    Idle,
    Loading,
    Ready(T),
}

impl<T> Load<T> {
    pub fn ready_mut(&mut self) -> Option<&mut T> {
        match self {
            Load::Ready(v) => Some(v),
            _ => None,
        }
    }
}

/// Plan items with a tick box each and a cursor.
pub struct Checklist {
    pub items: Vec<Item>,
    pub checked: Vec<bool>,
    pub selected: usize,
    pub offset: usize,
    pub warnings: Vec<String>,
}

impl Checklist {
    pub fn new(items: Vec<Item>, warnings: Vec<String>, checked: bool) -> Self {
        Self { checked: vec![checked; items.len()], items, selected: 0, offset: 0, warnings }
    }

    /// (ticked count, ticked bytes)
    pub fn stats(&self) -> (usize, u64) {
        self.items.iter().zip(&self.checked).filter(|(_, c)| **c).fold((0, 0), |(n, s), (i, _)| (n + 1, s + i.size))
    }

    pub fn plan(&self) -> Plan {
        Plan {
            items: self.items.iter().zip(&self.checked).filter(|(_, c)| **c).map(|(i, _)| i.clone()).collect(),
            warnings: self.warnings.clone(),
        }
    }

    pub fn toggle_all(&mut self) {
        let all = self.checked.iter().all(|c| *c);
        self.checked.fill(!all);
    }
}

pub struct Reclaim {
    pub rows: Vec<CatRow>,
    pub list: Checklist,
}

pub struct AppsView {
    pub apps: Vec<AppInfo>,
    pub selected: usize,
    pub offset: usize,
    pub by_size: bool,
}

impl AppsView {
    pub fn sort(&mut self) {
        if self.by_size {
            self.apps.sort_by_key(|a| std::cmp::Reverse(a.size));
        } else {
            self.apps.sort_by_key(|a| a.name.to_lowercase());
        }
        self.selected = 0;
        self.offset = 0;
    }
}

pub enum Dialog {
    Confirm { title: String, lines: Vec<String>, plan: Plan, mode: Mode, apps: bool, stage: u8 },
    Result { title: String, lines: Vec<String> },
    Busy(String),
    Help,
}
