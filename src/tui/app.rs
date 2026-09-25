//! TUI state and its update function. No terminal or threads in here, so it is unit-testable:
//! input becomes an `Action`, `act` mutates state and returns `Effect`s for the runner to perform.

use ratatui::crossterm::event::{MouseButton, MouseEventKind};

use super::hit::HitMap;
use super::model::*;
use super::theme::Theme;
use crate::core::executor::Mode;
use crate::core::plan::Plan;
use crate::core::scanner::format_size;

pub struct App {
    pub screen: Screen,
    pub theme: Theme,
    pub quit: bool,
    pub cleanup: Load<Reclaim>,
    pub apps: Load<AppsView>,
    pub leftovers: Load<Checklist>,
    pub large: Load<Checklist>,
    pub dupes: Load<Checklist>,
    pub dialog: Option<Dialog>,
    pub permanent: bool,
    /// (used, total) bytes of the volume holding the home directory.
    pub disk: Option<(u64, u64)>,
    pub hits: HitMap,
    /// Rows visible in the current list, set by the last draw; used for page moves.
    pub view_rows: usize,
    pub tick: usize,
}

fn clamp_move(sel: usize, delta: isize, len: usize) -> usize {
    if len == 0 { 0 } else { (sel as isize).saturating_add(delta).clamp(0, len as isize - 1) as usize }
}

impl App {
    pub fn new(theme: Theme) -> Self {
        Self {
            screen: Screen::Dashboard,
            theme,
            quit: false,
            cleanup: Load::Idle,
            apps: Load::Idle,
            leftovers: Load::Idle,
            large: Load::Idle,
            dupes: Load::Idle,
            dialog: None,
            permanent: false,
            disk: None,
            hits: HitMap::default(),
            view_rows: 10,
            tick: 0,
        }
    }

    pub fn start(&mut self) -> Vec<Effect> {
        self.ensure(self.screen.kind())
    }

    pub fn checklist(&self) -> Option<&Checklist> {
        match self.screen {
            Screen::Cleanup => match &self.cleanup {
                Load::Ready(r) => Some(&r.list),
                _ => None,
            },
            Screen::Leftovers => ready(&self.leftovers),
            Screen::Large => ready(&self.large),
            Screen::Dupes => ready(&self.dupes),
            _ => None,
        }
    }

    fn list_mut(&mut self) -> Option<&mut Checklist> {
        match self.screen {
            Screen::Cleanup => self.cleanup.ready_mut().map(|r| &mut r.list),
            Screen::Leftovers => self.leftovers.ready_mut(),
            Screen::Large => self.large.ready_mut(),
            Screen::Dupes => self.dupes.ready_mut(),
            _ => None,
        }
    }

    fn is_idle(&self, kind: ScanKind) -> bool {
        match kind {
            ScanKind::Cleanup => matches!(self.cleanup, Load::Idle),
            ScanKind::Apps => matches!(self.apps, Load::Idle),
            ScanKind::Leftovers => matches!(self.leftovers, Load::Idle),
            ScanKind::Large => matches!(self.large, Load::Idle),
            ScanKind::Dupes => matches!(self.dupes, Load::Idle),
        }
    }

    fn is_loading(&self, kind: ScanKind) -> bool {
        match kind {
            ScanKind::Cleanup => matches!(self.cleanup, Load::Loading),
            ScanKind::Apps => matches!(self.apps, Load::Loading),
            ScanKind::Leftovers => matches!(self.leftovers, Load::Loading),
            ScanKind::Large => matches!(self.large, Load::Loading),
            ScanKind::Dupes => matches!(self.dupes, Load::Loading),
        }
    }

    fn set_loading(&mut self, kind: ScanKind) {
        match kind {
            ScanKind::Cleanup => self.cleanup = Load::Loading,
            ScanKind::Apps => self.apps = Load::Loading,
            ScanKind::Leftovers => self.leftovers = Load::Loading,
            ScanKind::Large => self.large = Load::Loading,
            ScanKind::Dupes => self.dupes = Load::Loading,
        }
    }

    /// Starts a scan the first time a screen is shown.
    fn ensure(&mut self, kind: ScanKind) -> Vec<Effect> {
        if self.is_idle(kind) {
            self.set_loading(kind);
            vec![Effect::Scan(kind)]
        } else {
            vec![]
        }
    }

    fn switch(&mut self, screen: Screen) -> Vec<Effect> {
        self.screen = screen;
        self.ensure(screen.kind())
    }

    fn refresh(&mut self) -> Vec<Effect> {
        let kind = self.screen.kind();
        if self.is_loading(kind) {
            return vec![];
        }
        self.set_loading(kind);
        vec![Effect::Scan(kind)]
    }

    fn move_sel(&mut self, delta: isize) {
        match self.screen {
            Screen::Apps => {
                if let Load::Ready(v) = &mut self.apps {
                    v.selected = clamp_move(v.selected, delta, v.apps.len());
                }
            }
            _ => {
                if let Some(l) = self.list_mut() {
                    l.selected = clamp_move(l.selected, delta, l.items.len());
                }
            }
        }
    }

    fn set_sel(&mut self, i: usize) {
        match self.screen {
            Screen::Apps => {
                if let Load::Ready(v) = &mut self.apps {
                    v.selected = i.min(v.apps.len().saturating_sub(1));
                }
            }
            _ => {
                if let Some(l) = self.list_mut() {
                    l.selected = i.min(l.items.len().saturating_sub(1));
                }
            }
        }
    }

    fn mode(&self) -> Mode {
        if self.permanent { Mode::Permanent } else { Mode::Trash }
    }

    fn confirm_dialog(&self, plan: Plan, apps: bool) -> Dialog {
        let mode = self.mode();
        let mut lines = vec![
            format!("{} item(s), {}", plan.items.len(), format_size(plan.total_size())),
            match mode {
                Mode::Permanent => "PERMANENT: items are deleted and cannot be recovered.".to_string(),
                _ => "Items are moved to the Trash.".to_string(),
            },
            String::new(),
        ];
        lines.extend(plan.items.iter().take(6).map(|i| format!("{:>9}  {}", format_size(i.size), i.path.display())));
        if plan.items.len() > 6 {
            lines.push(format!("…and {} more", plan.items.len() - 6));
        }
        lines.extend(plan.warnings.iter().take(3).map(|w| format!("note: {w}")));
        Dialog::Confirm { title: "Confirm".into(), lines, plan, mode, apps, stage: 0 }
    }

    fn info(title: &str, lines: Vec<String>) -> Dialog {
        Dialog::Result { title: title.into(), lines }
    }

    fn primary(&mut self) -> Vec<Effect> {
        if self.screen == Screen::Dashboard {
            return self.switch(Screen::Cleanup);
        }
        if self.screen == Screen::Apps {
            let app = match &self.apps {
                Load::Ready(v) => v.apps.get(v.selected).cloned(),
                _ => None,
            };
            return match app {
                Some(app) => {
                    self.dialog = Some(Dialog::Busy(format!("Checking {}…", app.name)));
                    vec![Effect::PlanUninstall(app)]
                }
                None => vec![],
            };
        }
        let Some(list) = self.checklist() else { return vec![] };
        let plan = list.plan();
        self.dialog = Some(if plan.items.is_empty() {
            Self::info("Nothing selected", vec!["Tick items with space or a click first.".into()])
        } else {
            self.confirm_dialog(plan, false)
        });
        vec![]
    }

    pub fn act(&mut self, action: Action) -> Vec<Effect> {
        match action {
            Action::Quit => self.quit = true,
            Action::Tab(s) => return self.switch(s),
            Action::NextTab => return self.switch(Screen::ALL[(self.screen.index() + 1) % Screen::ALL.len()]),
            Action::PrevTab => {
                return self.switch(Screen::ALL[(self.screen.index() + Screen::ALL.len() - 1) % Screen::ALL.len()]);
            }
            Action::Move(d) => self.move_sel(d),
            Action::Row(i) => self.set_sel(i),
            Action::Toggle(i) => {
                self.set_sel(i);
                if let Some(c) = self.list_mut().and_then(|l| l.checked.get_mut(i)) {
                    *c = !*c;
                }
            }
            Action::ToggleCurrent => {
                if let Some(l) = self.list_mut() {
                    let i = l.selected;
                    if let Some(c) = l.checked.get_mut(i) {
                        *c = !*c;
                    }
                }
            }
            Action::SelectAll => {
                if let Some(l) = self.list_mut() {
                    l.toggle_all();
                }
            }
            Action::Refresh => return self.refresh(),
            Action::Sort => {
                if let Load::Ready(v) = &mut self.apps {
                    v.by_size = !v.by_size;
                    v.sort();
                }
            }
            Action::Primary => return self.primary(),
            Action::TogglePermanent => self.permanent = !self.permanent,
            Action::Help => self.dialog = Some(Dialog::Help),
            Action::Cancel => {
                if !matches!(self.dialog, Some(Dialog::Busy(_))) {
                    self.dialog = None;
                }
            }
            Action::Confirm => {
                if let Some(Dialog::Confirm { plan, mode, apps, stage, .. }) = self.dialog.take_if(|d| matches!(d, Dialog::Confirm { .. })) {
                    if mode == Mode::Permanent && stage == 0 {
                        self.dialog = Some(Dialog::Confirm {
                            title: "Are you sure?".into(),
                            lines: vec![
                                format!("Delete {} item(s) permanently?", plan.items.len()),
                                "PERMANENT: this cannot be undone.".into(),
                            ],
                            plan,
                            mode,
                            apps,
                            stage: 1,
                        });
                    } else {
                        self.dialog = Some(Dialog::Busy("Removing…".into()));
                        return vec![Effect::Execute { plan, mode, apps }];
                    }
                } else {
                    return self.act(Action::Cancel);
                }
            }
        }
        vec![]
    }


    pub fn update(&mut self, msg: Msg) -> Vec<Effect> {
        match msg {
            Msg::Tick => {
                self.tick = self.tick.wrapping_add(1);
                vec![]
            }
            Msg::Key(k) => self.key_action(k).map(|a| self.act(a)).unwrap_or_default(),
            Msg::Mouse(m) => match m.kind {
                MouseEventKind::Down(MouseButton::Left) => self.hits.at(m.column, m.row).map(|a| self.act(a)).unwrap_or_default(),
                MouseEventKind::ScrollUp if self.dialog.is_none() => self.act(Action::Move(-3)),
                MouseEventKind::ScrollDown if self.dialog.is_none() => self.act(Action::Move(3)),
                _ => vec![],
            },
            Msg::Disk(d) => {
                self.disk = d;
                vec![]
            }
            Msg::Scanned(r) => {
                self.scanned(r);
                vec![]
            }
            Msg::Planned(res) => {
                self.dialog = Some(match res {
                    Ok(plan) => self.confirm_dialog(plan, true),
                    Err(e) => Self::info("Cannot uninstall", vec![e]),
                });
                vec![]
            }
            Msg::Executed(report, mode) => {
                let verb = if mode == Mode::Permanent { "Deleted" } else { "Moved to Trash" };
                let mut lines = vec![format!("{verb}: {} item(s), {} freed", report.removed.len(), format_size(report.freed()))];
                if !report.skipped.is_empty() {
                    lines.push(format!("{} skipped:", report.skipped.len()));
                    lines.extend(report.skipped.iter().take(5).map(|(p, why)| format!("  {}: {why}", p.display())));
                }
                self.dialog = Some(Self::info("Done", lines));
                self.cleanup = Load::Idle;
                self.apps = Load::Idle;
                self.leftovers = Load::Idle;
                self.large = Load::Idle;
                self.dupes = Load::Idle;
                self.ensure(self.screen.kind())
            }
        }
    }

    fn scanned(&mut self, result: ScanResult) {
        match result {
            ScanResult::Cleanup(rows) => {
                let mut items = Vec::new();
                let mut warnings = Vec::new();
                let mut summary = Vec::new();
                for (id, _, plan) in rows {
                    summary.push(CatRow { id, size: plan.total_size(), count: plan.items.len() });
                    warnings.extend(plan.warnings);
                    if !id.is_trash() {
                        items.extend(plan.items);
                    }
                }
                items.sort_by_key(|i| std::cmp::Reverse(i.size));
                self.cleanup = Load::Ready(Reclaim { rows: summary, list: Checklist::new(items, warnings, true) });
            }
            ScanResult::Apps(apps) => {
                let mut v = AppsView { apps, selected: 0, offset: 0, by_size: false };
                v.sort();
                self.apps = Load::Ready(v);
            }
            ScanResult::List(kind, plan) => {
                let list = Checklist::new(plan.items, plan.warnings, false);
                match kind {
                    ScanKind::Leftovers => self.leftovers = Load::Ready(list),
                    ScanKind::Large => self.large = Load::Ready(list),
                    ScanKind::Dupes => self.dupes = Load::Ready(list),
                    ScanKind::Cleanup | ScanKind::Apps => {}
                }
            }
        }
    }
}

fn ready<T>(l: &Load<T>) -> Option<&T> {
    match l {
        Load::Ready(v) => Some(v),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::executor::Report;
    use crate::core::plan::{Category, Item};
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

    fn item(name: &str, size: u64) -> Item {
        Item { path: format!("/tmp/{name}").into(), size, category: Category::Caches, reason: "r".into() }
    }

    fn key(c: KeyCode) -> Msg {
        Msg::Key(KeyEvent::new_with_kind(c, KeyModifiers::NONE, KeyEventKind::Press))
    }

    fn with_leftovers() -> App {
        let mut app = App::new(Theme { truecolor: false });
        app.screen = Screen::Leftovers;
        app.leftovers = Load::Loading;
        let plan = Plan { items: vec![item("a", 10), item("b", 20)], warnings: vec![] };
        app.update(Msg::Scanned(ScanResult::List(ScanKind::Leftovers, plan)));
        app
    }

    #[test]
    fn first_visit_scans_once() {
        let mut app = App::new(Theme { truecolor: false });
        assert!(matches!(app.start().as_slice(), [Effect::Scan(ScanKind::Cleanup)]));
        assert!(matches!(app.act(Action::Tab(Screen::Apps)).as_slice(), [Effect::Scan(ScanKind::Apps)]));
        assert!(app.act(Action::Tab(Screen::Dashboard)).is_empty());
        assert!(app.act(Action::Tab(Screen::Apps)).is_empty());
    }

    #[test]
    fn number_keys_and_tab_switch_screens() {
        let mut app = App::new(Theme { truecolor: false });
        app.update(key(KeyCode::Char('4')));
        assert_eq!(app.screen, Screen::Leftovers);
        app.update(key(KeyCode::Tab));
        assert_eq!(app.screen, Screen::Large);
        app.update(key(KeyCode::BackTab));
        app.update(key(KeyCode::BackTab));
        assert_eq!(app.screen, Screen::Cleanup);
    }

    #[test]
    fn toggle_select_all_and_stats() {
        let mut app = with_leftovers();
        app.act(Action::Toggle(1));
        assert_eq!(app.checklist().unwrap().stats(), (1, 20));
        app.act(Action::SelectAll);
        assert_eq!(app.checklist().unwrap().stats(), (2, 30));
        app.act(Action::SelectAll);
        assert_eq!(app.checklist().unwrap().stats(), (0, 0));
    }

    #[test]
    fn nothing_selected_does_not_execute() {
        let mut app = with_leftovers();
        assert!(app.act(Action::Primary).is_empty());
        assert!(matches!(app.dialog, Some(Dialog::Result { .. })));
    }

    #[test]
    fn trash_confirm_executes_selected_only() {
        let mut app = with_leftovers();
        app.act(Action::Toggle(0));
        assert!(app.act(Action::Primary).is_empty());
        let fx = app.act(Action::Confirm);
        match fx.as_slice() {
            [Effect::Execute { plan, mode: Mode::Trash, apps: false }] => {
                assert_eq!(plan.items.len(), 1);
                assert_eq!(plan.items[0].path.to_str(), Some("/tmp/a"));
            }
            other => panic!("unexpected effects: {other:?}"),
        }
        assert!(matches!(app.dialog, Some(Dialog::Busy(_))));
    }

    #[test]
    fn permanent_needs_two_confirmations() {
        let mut app = with_leftovers();
        app.act(Action::SelectAll);
        app.act(Action::TogglePermanent);
        app.act(Action::Primary);
        assert!(app.act(Action::Confirm).is_empty(), "first confirm only escalates");
        assert!(matches!(app.dialog, Some(Dialog::Confirm { stage: 1, .. })));
        assert!(matches!(app.act(Action::Confirm).as_slice(), [Effect::Execute { mode: Mode::Permanent, .. }]));
    }

    #[test]
    fn cancel_never_executes_and_busy_cannot_be_dismissed() {
        let mut app = with_leftovers();
        app.act(Action::SelectAll);
        app.act(Action::Primary);
        assert!(app.act(Action::Cancel).is_empty());
        assert!(app.dialog.is_none());
        app.dialog = Some(Dialog::Busy("x".into()));
        app.act(Action::Cancel);
        assert!(app.dialog.is_some());
    }

    #[test]
    fn executed_invalidates_and_rescans_current_screen() {
        let mut app = with_leftovers();
        let fx = app.update(Msg::Executed(Report::default(), Mode::Trash));
        assert!(matches!(fx.as_slice(), [Effect::Scan(ScanKind::Leftovers)]));
        assert!(matches!(app.dialog, Some(Dialog::Result { .. })));
    }

    #[test]
    fn moves_are_clamped() {
        let mut app = with_leftovers();
        app.act(Action::Move(100));
        assert_eq!(app.checklist().unwrap().selected, 1);
        app.act(Action::Move(isize::MIN / 2));
        assert_eq!(app.checklist().unwrap().selected, 0);
    }

    #[test]
    fn refresh_ignored_while_loading() {
        let mut app = App::new(Theme { truecolor: false });
        app.start();
        assert!(app.act(Action::Refresh).is_empty());
    }
}
