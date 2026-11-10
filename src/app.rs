//! The window: tabs, the tab bar, keyboard and mouse, and the terminal view.

use crate::browse::{self, Action, Browse};
use crate::commands::{self, Cmd};
use crate::find::Find;
use crate::input::{is_app_shortcut, key_bytes, paste_bytes};
use crate::panes::{self, Dir, Node};
use crate::session::Session;
use crate::settings::{self, Settings};
use crate::shell::ShellSpec;
use crate::themes::{self, Theme};
use crate::view::{self, Look};
use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::selection::{Selection, SelectionType};
use alacritty_terminal::term::TermMode;
use egui::{Color32, Key, RichText, Sense, Vec2};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// One shell in a tab (a tab has one, or more when split).
pub struct Pane {
    pub id: u64,
    pub session: Session,
    /// Its size in cells, as last laid out.
    grid: (u16, u16),
    /// Browse mode (the free-floating cursor), when on.
    pub browse: Option<Browse>,
    /// Something was typed since the last prompt (then ↑ is shell history).
    typed: bool,
    /// Prompts seen when `typed` was last reset.
    prompts: u64,
}

/// A tab: one pane, or several split.
pub struct Tab {
    root: Option<Node<Pane>>,
    /// The pane with the focus.
    pub focus: u64,
}

impl Tab {
    fn new(pane: Pane) -> Tab {
        Tab {
            focus: pane.id,
            root: Some(Node::Leaf(pane)),
        }
    }

    pub fn tree(&self) -> &Node<Pane> {
        self.root.as_ref().expect("a tab always has a pane")
    }

    fn tree_mut(&mut self) -> &mut Node<Pane> {
        self.root.as_mut().expect("a tab always has a pane")
    }

    /// The pane with the focus.
    pub fn pane(&self) -> &Pane {
        let f = self.focus;
        let t = self.tree();
        t.find(&|p: &Pane| p.id == f)
            .unwrap_or_else(|| t.leaves()[0])
    }

    pub fn pane_mut(&mut self) -> &mut Pane {
        let f = self.focus;
        if self.tree().find(&|p: &Pane| p.id == f).is_none() {
            self.focus = self.tree().leaves()[0].id;
        }
        let f = self.focus;
        self.tree_mut().find_mut(&|p: &Pane| p.id == f).unwrap()
    }

    pub fn panes(&self) -> Vec<&Pane> {
        self.tree().leaves()
    }

    /// Splits the focused pane; the new one gets the focus.
    fn split(&mut self, dir: Dir, pane: Pane) {
        let (f, id) = (self.focus, pane.id);
        let root = self.root.take().unwrap();
        let mut new = Some(pane);
        self.root = Some(root.split(&|p: &Pane| p.id == f, dir, &mut new));
        if new.is_none() {
            self.focus = id;
        }
    }

    /// Closes a pane (dropping it ends its shell). False when it was the
    /// tab's last one: then the tab should close.
    fn close_pane(&mut self, id: u64) -> bool {
        let order: Vec<u64> = self.panes().iter().map(|p| p.id).collect();
        let root = self.root.take().unwrap();
        let (rest, gone) = root.remove(&|p: &Pane| p.id == id);
        drop(gone);
        match rest {
            None => false,
            Some(r) => {
                self.root = Some(r);
                if self.focus == id {
                    // The focus goes to the pane before it (or the first).
                    let i = order.iter().position(|p| *p == id).unwrap_or(0);
                    let left: Vec<u64> = self.panes().iter().map(|p| p.id).collect();
                    self.focus = left[i.saturating_sub(1).min(left.len() - 1)];
                }
                true
            }
        }
    }

    /// "Home", "jamalterm", "/" for the focused pane.
    pub fn folder(&self, home: &str) -> String {
        self.pane().folder(home)
    }

    pub fn program(&self, shell_name: &str) -> String {
        self.pane().program(shell_name)
    }
}

impl Pane {
    /// "Home", "jamalterm", "/" — the folder in words (never just "~").
    pub fn folder(&self, home: &str) -> String {
        folder_name(&self.session.screen.lock().unwrap().cwd, home)
    }

    /// The program running now, else the shell.
    pub fn program(&self, shell_name: &str) -> String {
        let s = self.session.screen.lock().unwrap();
        s.commands
            .running()
            .and_then(|b| program_of(&b.command))
            .unwrap_or_else(|| shell_name.to_string())
    }
}

/// The folder for a tab name: "Home" for the home folder, else its last part.
pub fn folder_name(cwd: &str, home: &str) -> String {
    if cwd.is_empty() {
        return String::new();
    }
    if cwd == home {
        return "Home".into();
    }
    if cwd == "/" {
        return "Computer root".into();
    }
    cwd.trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or(cwd)
        .to_string()
}

/// The program a command runs ("sudo npm run build" → "npm").
pub fn program_of(command: &str) -> Option<String> {
    let words: Vec<&str> = command
        .split_whitespace()
        .filter(|w| !w.contains('=') || w.starts_with('-'))
        .collect();
    let first = match words.first() {
        Some(&"sudo") | Some(&"time") => words.get(1),
        w => w,
    }?;
    Some(first.rsplit('/').next().unwrap_or(first).to_string())
}

/// "Home — zsh": a window or tab name in words.
pub fn tab_name(folder: &str, program: &str) -> String {
    if folder.is_empty() {
        program.to_string()
    } else {
        format!("{folder} — {program}")
    }
}

pub struct App {
    tabs: Vec<Tab>,
    /// The next pane's id.
    next_id: u64,
    active: usize,
    settings: Settings,
    settings_path: PathBuf,
    theme: Theme,
    grid: (u16, u16),
    home: String,
    shell_name: String,
    selecting: bool,
    started: Instant,
    smoke: Option<crate::smoke::Smoke>,
    /// The find bar (⌘F), when open.
    find: Option<Find>,
    focus_find: bool,
    /// Where each pane of the tab in front was last drawn (for ⌥⌘ arrows).
    pane_rects: Vec<(u64, egui::Rect)>,
    /// The divider being dragged.
    dragging: Option<panes::Divider>,
    #[cfg(target_os = "macos")]
    menu: Option<crate::menu::MacMenu>,
}

impl App {
    pub fn new(cc: &eframe::CreationContext) -> Self {
        crate::fonts::install(&cc.egui_ctx);
        let settings_path = std::env::var_os("JAMALTERM_SETTINGS")
            .map(PathBuf::from)
            .unwrap_or_else(settings::default_path);
        let settings = settings::load(&settings_path);
        let theme = themes::by_id(&settings.theme);
        let shell = if settings.shell.is_empty() {
            crate::shell::default_shell()
        } else {
            settings.shell.clone()
        };
        let mut app = App {
            tabs: vec![],
            next_id: 1,
            active: 0,
            theme,
            grid: (80, 24),
            home: dirs::home_dir()
                .map(|h| h.display().to_string())
                .unwrap_or_default(),
            shell_name: shell.rsplit('/').next().unwrap_or("shell").to_string(),
            settings,
            settings_path,
            selecting: false,
            started: Instant::now(),
            smoke: crate::smoke::Smoke::from_env(),
            find: None,
            focus_find: false,
            pane_rects: vec![],
            dragging: None,
            #[cfg(target_os = "macos")]
            menu: crate::menu::MacMenu::install(&cc.egui_ctx),
        };
        #[cfg(target_os = "macos")]
        if let Some(s) = app.smoke.as_mut() {
            s.menu_bar = Some(app.menu.is_some());
        }
        app.new_tab(&cc.egui_ctx, None);
        app
    }

    /// Starts a shell for a new tab or pane, in the focused pane's folder
    /// (like Terminal), or in `dir`.
    fn spawn_pane(&mut self, ctx: &egui::Context, dir: Option<PathBuf>) -> Option<Pane> {
        let (dir, grid) = match self.pane() {
            Some(p) => {
                let cwd = p.session.screen.lock().unwrap().cwd.clone();
                (
                    dir.or_else(|| (!cwd.is_empty()).then(|| PathBuf::from(cwd))),
                    p.grid,
                )
            }
            None => (dir, self.grid),
        };
        let spec = ShellSpec {
            shell: self.settings.shell.clone(),
            args: None,
            dir,
            integration: self.settings.shell_integration,
            extra_env: vec![],
        };
        let ctx2 = ctx.clone();
        let repaint: Arc<dyn Fn() + Send + Sync> = Arc::new(move || ctx2.request_repaint());
        match Session::spawn(&spec, grid.0, grid.1, self.settings.scrollback, repaint) {
            Ok(session) => {
                self.next_id += 1;
                Some(Pane {
                    id: self.next_id,
                    session,
                    grid,
                    browse: None,
                    typed: false,
                    prompts: 0,
                })
            }
            Err(e) => {
                eprintln!("jamalterm: the shell didn't start: {e}");
                None
            }
        }
    }

    fn new_tab(&mut self, ctx: &egui::Context, dir: Option<PathBuf>) {
        if let Some(p) = self.spawn_pane(ctx, dir) {
            self.tabs.push(Tab::new(p));
            self.active = self.tabs.len() - 1;
        }
    }

    /// Splits the focused pane (⌘D right, ⇧⌘D down).
    fn split(&mut self, ctx: &egui::Context, dir: Dir) {
        if let Some(p) = self.spawn_pane(ctx, None)
            && let Some(t) = self.tabs.get_mut(self.active)
        {
            t.split(dir, p);
        }
    }

    /// ⌘W: closes the focused pane, or the tab when it isn't split.
    fn close_pane(&mut self, ctx: &egui::Context) {
        let Some(t) = self.tabs.get_mut(self.active) else {
            return;
        };
        let id = t.focus;
        if !t.close_pane(id) {
            self.close_tab(ctx, self.active);
        }
    }

    /// The focused pane of the tab in front.
    fn pane(&self) -> Option<&Pane> {
        self.tabs.get(self.active).map(|t| t.pane())
    }

    fn pane_mut(&mut self) -> Option<&mut Pane> {
        self.tabs.get_mut(self.active).map(|t| t.pane_mut())
    }

    /// Moves the focus to the next/previous pane (⌘] ⌘[) or the one in a
    /// direction (⌥⌘ arrows), using where the panes were last drawn.
    fn focus_pane(&mut self, step: i32, toward: Option<Vec2>) {
        let Some(t) = self.tabs.get_mut(self.active) else {
            return;
        };
        let ids: Vec<u64> = t.panes().iter().map(|p| p.id).collect();
        let Some(i) = ids.iter().position(|id| *id == t.focus) else {
            return;
        };
        let next = match toward {
            Some(v) => {
                let rects: Vec<egui::Rect> = self.pane_rects.iter().map(|(_, r)| *r).collect();
                let at = self.pane_rects.iter().position(|(id, _)| *id == t.focus);
                at.and_then(|at| panes::neighbour(&rects, at, v))
                    .map(|j| self.pane_rects[j].0)
            }
            None => Some(ids[(i as i64 + step as i64).rem_euclid(ids.len() as i64) as usize]),
        };
        if let Some(n) = next {
            t.focus = n;
        }
    }

    fn close_tab(&mut self, ctx: &egui::Context, i: usize) {
        if i >= self.tabs.len() {
            return;
        }
        self.tabs.remove(i); // dropping the session ends its shell
        if self.tabs.is_empty() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        self.active = self.active.min(self.tabs.len() - 1);
    }

    fn set_font_size(&mut self, size: u32) {
        self.settings.font_size = size.clamp(8, 48);
        let _ = settings::save(&self.settings_path, &self.settings);
    }

    /// ⌘ shortcuts (Ctrl+Shift on Linux). Returns true when handled.
    fn shortcut(&mut self, ctx: &egui::Context, key: Key, m: &egui::Modifiers) -> bool {
        if !is_app_shortcut(m) {
            return false;
        }
        // On Linux ⇧ is part of every shortcut, so it can't tell two apart.
        let shift = cfg!(target_os = "macos").then_some(m.shift);
        let Some(cmd) = commands::for_key(key, shift, m.alt) else {
            return false;
        };
        self.run(ctx, cmd);
        true
    }

    /// Runs a command, from a shortcut or the menu bar.
    fn run(&mut self, ctx: &egui::Context, cmd: Cmd) {
        match cmd {
            Cmd::NewTab => self.new_tab(ctx, None),
            Cmd::SplitRight => self.split(ctx, Dir::Right),
            Cmd::SplitDown => self.split(ctx, Dir::Down),
            Cmd::ClosePane => self.close_pane(ctx),
            Cmd::CloseTab => self.close_tab(ctx, self.active),
            Cmd::PreviousPane => self.focus_pane(-1, None),
            Cmd::NextPane => self.focus_pane(1, None),
            Cmd::PaneLeft => self.focus_pane(0, Some(Vec2::new(-1.0, 0.0))),
            Cmd::PaneRight => self.focus_pane(0, Some(Vec2::new(1.0, 0.0))),
            Cmd::PaneUp => self.focus_pane(0, Some(Vec2::new(0.0, -1.0))),
            Cmd::PaneDown => self.focus_pane(0, Some(Vec2::new(0.0, 1.0))),
            Cmd::Copy => {
                if let Some(tab) = self.pane()
                    && let Some(text) = tab
                        .session
                        .screen
                        .lock()
                        .unwrap()
                        .term
                        .selection_to_string()
                {
                    ctx.copy_text(text);
                }
            }
            Cmd::Paste => {
                let text = arboard::Clipboard::new().and_then(|mut c| c.get_text());
                if let Ok(t) = text {
                    if ctx.memory(|m| m.focused().is_some()) {
                        if let Some(f) = self.find.as_mut() {
                            f.query.push_str(t.lines().next().unwrap_or(""));
                        }
                    } else {
                        self.paste(&t);
                    }
                }
            }
            Cmd::Browse => {
                // ⌘↑ always starts browse mode.
                self.enter_browse(true);
            }
            Cmd::Find => {
                // Find, keeping the last text (like Safari).
                self.find.get_or_insert_with(Find::default);
                self.focus_find = true;
            }
            Cmd::FindNext | Cmd::FindPrevious => {
                if let Some(f) = self.find.as_mut() {
                    f.step(if cmd == Cmd::FindPrevious { -1 } else { 1 });
                    self.show_match();
                }
            }
            Cmd::Clear => {
                // Clear scrollback (and the screen), like Terminal's ⌘K.
                if let Some(t) = self.pane() {
                    t.session.write(b"\x0c");
                    let mut s = t.session.screen.lock().unwrap();
                    s.term.grid_mut().clear_history();
                    s.commands.blocks.clear();
                }
            }
            Cmd::Bigger => self.set_font_size(self.settings.font_size + 1),
            Cmd::Smaller => self.set_font_size(self.settings.font_size.saturating_sub(1)),
            Cmd::ActualSize => self.set_font_size(Settings::default().font_size),
            Cmd::PreviousTab => self.active = (self.active + self.tabs.len() - 1) % self.tabs.len(),
            Cmd::NextTab => self.active = (self.active + 1) % self.tabs.len(),
            Cmd::Tab(n) => {
                if (n as usize) <= self.tabs.len() {
                    self.active = n as usize - 1;
                }
            }
        }
    }

    /// Pastes text into the current tab's shell.
    fn paste(&mut self, t: &str) {
        if let Some(tab) = self.pane() {
            let bracketed = tab
                .session
                .screen
                .lock()
                .unwrap()
                .term
                .mode()
                .contains(TermMode::BRACKETED_PASTE);
            tab.session.write(&paste_bytes(t, bracketed));
        }
    }

    /// Selects the current match and scrolls to it.
    fn show_match(&mut self) {
        let Some(m) = self.find.as_ref().and_then(|f| f.current_match().cloned()) else {
            return;
        };
        let Some(tab) = self.pane() else {
            return;
        };
        let mut s = tab.session.screen.lock().unwrap();
        let mut sel = Selection::new(
            SelectionType::Simple,
            *m.start(),
            alacritty_terminal::index::Side::Left,
        );
        sel.update(*m.end(), alacritty_terminal::index::Side::Right);
        s.term.selection = Some(sel);
        // Bring its line into view.
        let rows = s.term.screen_lines() as i32;
        let offset = s.term.grid().display_offset() as i32;
        let line = m.start().line.0; // screen line when not scrolled; negative in the history
        let top = -offset;
        if line < top {
            s.term.scroll_display(Scroll::Delta(top - line + rows / 3));
        } else if line >= top + rows {
            s.term
                .scroll_display(Scroll::Delta(-(line - (top + rows) + 1 + rows / 3)));
        }
    }

    fn find_bar(&mut self, ui: &mut egui::Ui) {
        let mut close = false;
        let mut step = 0;
        let Some(f) = self.find.as_mut() else { return };
        ui.horizontal(|ui| {
            ui.label("Find");
            let edit = ui.add(
                egui::TextEdit::singleline(&mut f.query)
                    .desired_width(260.0)
                    .hint_text("text in the output"),
            );
            if self.focus_find {
                edit.request_focus();
                self.focus_find = false;
            }
            if edit.has_focus() || edit.lost_focus() {
                ui.input(|i| {
                    if i.key_pressed(Key::Enter) {
                        step = if i.modifiers.shift { -1 } else { 1 };
                    }
                    if i.key_pressed(Key::Escape) {
                        close = true;
                    }
                });
                if edit.lost_focus() && step != 0 {
                    edit.request_focus(); // Return steps through matches, keeping the box
                }
            }
            ui.label(f.status());
            if ui
                .small_button("‹")
                .on_hover_text("Previous (⇧⌘G)")
                .clicked()
            {
                step = -1;
            }
            if ui.small_button("›").on_hover_text("Next (⌘G)").clicked() {
                step = 1;
            }
            if ui
                .small_button("Done")
                .on_hover_text("Close (esc)")
                .clicked()
            {
                close = true;
            }
        });
        let changed = self
            .tabs
            .get(self.active)
            .is_some_and(|t| f.update(&t.pane().session.screen.lock().unwrap().term));
        if step != 0 {
            f.step(step);
        }
        if close {
            self.find = None;
            if let Some(t) = self.pane() {
                t.session.screen.lock().unwrap().term.selection = None;
            }
        } else if changed || step != 0 {
            self.show_match();
        }
    }

    /// Starts browse mode at the shell's cursor, then moves up one line
    /// (it was ↑ or ⌘↑ that started it). `forced`: ⌘↑, even mid-line.
    fn enter_browse(&mut self, forced: bool) -> bool {
        let Some(tab) = self.pane_mut() else {
            return false;
        };
        let s = tab.session.screen.lock().unwrap();
        if s.term.mode().contains(TermMode::ALT_SCREEN)
            || (!forced && (!s.prompt_seen || s.commands.running().is_some()))
        {
            return false;
        }
        let row = s.abs_cursor_line();
        let col = s.term.grid().cursor.point.column.0;
        drop(s);
        let mut b = Browse::new(row, col);
        b.row = (row - 1).max(0);
        tab.browse = Some(b);
        self.scroll_to_browse();
        true
    }

    /// Keeps the browse cursor on screen.
    fn scroll_to_browse(&mut self) {
        let Some(tab) = self.pane() else {
            return;
        };
        let Some(b) = &tab.browse else { return };
        let mut s = tab.session.screen.lock().unwrap();
        let history = s.term.grid().history_size() as i64;
        let rows = s.term.screen_lines() as i64;
        let top = history - s.term.grid().display_offset() as i64;
        if b.row < top {
            s.term.scroll_display(Scroll::Delta((top - b.row) as i32));
        } else if b.row >= top + rows {
            s.term
                .scroll_display(Scroll::Delta(-((b.row - (top + rows - 1)) as i32)));
        }
    }

    /// A key while browsing. Returns false when the key should go to the
    /// shell after all (typing ends browse mode).
    fn browse_key(&mut self, ctx: &egui::Context, key: Key, m: &egui::Modifiers) -> bool {
        let vi = self.settings.prompt_keys == "vi";
        let confirm = self.settings.confirm_rerun;
        let Some(tab) = self.pane_mut() else {
            return true;
        };
        let s = tab.session.screen.lock().unwrap();
        let blocks = s.commands.blocks.clone();
        let bottom = s.abs_cursor_line();
        let cols = s.term.columns();
        let cwd = s.cwd.clone();
        drop(s);
        let view = browse::View {
            blocks: &blocks,
            bottom,
            cols,
            cwd: &cwd,
            confirm_rerun: confirm,
        };
        let Some(b) = tab.browse.as_mut() else {
            return true;
        };
        let row = b.row;
        let action = b.key(key, m, &view);
        let block = browse::block_at(&blocks, row, bottom).map(|i| blocks[i].clone());
        match action {
            Action::Stay => {}
            Action::Exit => tab.browse = None,
            Action::ExitAndPass => {
                tab.browse = None;
                return false;
            }
            Action::Run { command, cd } => {
                tab.browse = None;
                tab.session
                    .screen
                    .lock()
                    .unwrap()
                    .term
                    .scroll_display(Scroll::Bottom);
                tab.session
                    .write(&browse::type_command(&command, cd.as_deref(), true, vi));
            }
            Action::Edit(command) => {
                tab.browse = None;
                tab.session
                    .screen
                    .lock()
                    .unwrap()
                    .term
                    .scroll_display(Scroll::Bottom);
                tab.session
                    .write(&browse::type_command(&command, None, false, vi));
                tab.typed = true;
            }
            Action::CopyCommand => {
                let s = tab.session.screen.lock().unwrap();
                let text = match &block {
                    Some(b) if !b.command.is_empty() => b.command.clone(),
                    _ => s.line_text(row),
                };
                ctx.copy_text(text);
            }
            Action::CopyOutput => {
                if let Some(b) = &block {
                    let s = tab.session.screen.lock().unwrap();
                    let from = b.start.unwrap_or(b.prompt + 1);
                    let to = browse::block_end(b, bottom);
                    let out: Vec<String> = (from..=to).map(|l| s.line_text(l)).collect();
                    ctx.copy_text(out.join("\n").trim_end().to_string());
                }
            }
        }
        self.scroll_to_browse();
        true
    }

    fn tab_bar(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        let mut select = None;
        let mut close = None;
        ui.horizontal(|ui| {
            for (i, tab) in self.tabs.iter().enumerate() {
                let screen = tab.pane().session.screen.lock().unwrap();
                let failed = screen.commands.running().is_none()
                    && screen
                        .commands
                        .last_finished()
                        .is_some_and(|b| b.exit != Some(0));
                let running = screen.commands.running().is_some();
                drop(screen);
                let mut name = tab_name(&tab.folder(&self.home), &tab.program(&self.shell_name));
                let n = tab.tree().count();
                if n > 1 {
                    name = format!("{name} · {n} panes"); // in words, not a symbol
                }
                // Status in shape and words: "!" failed, "…" running.
                let mark = if failed {
                    "! "
                } else if running {
                    "… "
                } else {
                    ""
                };
                let mut text = RichText::new(format!("{}  {mark}{name}", i + 1));
                if i == self.active {
                    text = text.strong();
                }
                let b = ui.add(
                    egui::Button::new(text)
                        .selected(i == self.active)
                        .min_size(Vec2::new(140.0, 24.0)),
                );
                if b.clicked() {
                    select = Some(i);
                }
                if b.middle_clicked() {
                    close = Some(i);
                }
                if ui
                    .small_button("×")
                    .on_hover_text("Close tab (⌘W)")
                    .clicked()
                {
                    close = Some(i);
                }
            }
            if ui.button("+").on_hover_text("New tab (⌘T)").clicked() {
                self.new_tab(ctx, None);
            }
        });
        if let Some(i) = select {
            self.active = i;
        }
        if let Some(i) = close {
            self.close_tab(ctx, i);
        }
    }
}

impl eframe::App for App {
    /// The self-test's key presses arrive as if typed.
    fn raw_input_hook(&mut self, _ctx: &egui::Context, raw: &mut egui::RawInput) {
        if let Some(s) = self.smoke.as_mut() {
            raw.events.append(&mut s.keys);
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        if self.tabs.is_empty() {
            return;
        }
        let font_size = self.settings.font_size as f32;
        let cell = view::cell_size(&ctx, font_size, self.settings.line_height);

        // A new prompt: nothing typed on it yet.
        for t in &mut self.tabs {
            for pane in t.tree_mut().leaves_mut() {
                let p = pane.session.screen.lock().unwrap().commands.prompts;
                if p != pane.prompts {
                    pane.prompts = p;
                    pane.typed = false;
                }
            }
        }

        // Menu bar choices (and their key equivalents) on a Mac.
        #[cfg(target_os = "macos")]
        for cmd in self.menu.as_ref().map(|m| m.chosen()).unwrap_or_default() {
            self.run(&ctx, cmd);
        }
        if self.tabs.is_empty() {
            return;
        }

        // Keyboard: app shortcuts first, then browse mode, then the shell.
        let mut swallow_text = false;
        let typing_in_field = ctx.memory(|m| m.focused().is_some());
        let events = ctx.input(|i| i.events.clone());
        for ev in events {
            match ev {
                egui::Event::Key {
                    key,
                    pressed: true,
                    modifiers,
                    ..
                } => {
                    if self.shortcut(&ctx, key, &modifiers) {
                        swallow_text = true;
                        continue;
                    }
                    if typing_in_field {
                        continue; // the find box has the keys
                    }
                    // Browse mode takes the keys while it's on.
                    if self
                        .tabs
                        .get(self.active)
                        .is_some_and(|t| t.pane().browse.is_some())
                    {
                        if self.browse_key(&ctx, key, &modifiers) {
                            swallow_text = true;
                            continue;
                        }
                    } else if key == Key::ArrowUp
                        && modifiers.is_none()
                        && self.settings.arrow_browse
                        && self.settings.shell_integration
                        && self.pane().is_some_and(|t| !t.typed)
                        && self.enter_browse(false)
                    {
                        // ↑ at an empty prompt: browse mode.
                        continue;
                    }
                    let option_meta = self.settings.option_is_meta;
                    let Some(tab) = self.pane_mut() else {
                        break;
                    };
                    let app_cursor = tab
                        .session
                        .screen
                        .lock()
                        .unwrap()
                        .term
                        .mode()
                        .contains(TermMode::APP_CURSOR);
                    if let Some(bytes) = key_bytes(key, &modifiers, app_cursor, option_meta) {
                        let mut s = tab.session.screen.lock().unwrap();
                        s.term.scroll_display(Scroll::Bottom); // typing jumps back to the prompt
                        s.term.selection = None;
                        drop(s);
                        tab.session.write(&bytes);
                        if key != Key::ArrowUp && key != Key::ArrowDown && key != Key::Enter {
                            tab.typed = true;
                        }
                    }
                }
                egui::Event::Text(t) => {
                    if swallow_text || typing_in_field {
                        continue; // the key already did something (a shortcut, browse mode)
                    }
                    if let Some(tab) = self.pane_mut() {
                        tab.typed = true;
                        tab.session
                            .screen
                            .lock()
                            .unwrap()
                            .term
                            .scroll_display(Scroll::Bottom);
                        tab.session.write(t.as_bytes());
                    }
                }
                egui::Event::Paste(t) => {
                    if !typing_in_field {
                        self.paste(&t);
                    }
                }
                egui::Event::Copy if !typing_in_field => self.run(&ctx, Cmd::Copy),
                _ => {}
            }
        }
        if self.tabs.is_empty() {
            return;
        }

        if self.tabs.len() > 1 || self.settings.show_tab_bar {
            egui::Panel::top("tabs").show(ui, |ui| self.tab_bar(&ctx, ui));
        }
        if self.tabs.is_empty() {
            return;
        }

        if self.find.is_some() {
            egui::Panel::top("find").show(ui, |ui| self.find_bar(ui));
        }

        // Browse mode's key bar.
        if let Some(tab) = self.pane()
            && let Some(b) = &tab.browse
        {
            let s = tab.session.screen.lock().unwrap();
            let view = browse::View {
                blocks: &s.commands.blocks,
                bottom: s.abs_cursor_line(),
                cols: 0,
                cwd: &s.cwd,
                confirm_rerun: false,
            };
            let (status, keys) = b.hint(&view);
            drop(s);
            egui::Panel::bottom("browse-keys").show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(status).monospace());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        for (k, what) in keys.iter().rev() {
                            ui.label(*what);
                            ui.label(
                                RichText::new(*k)
                                    .monospace()
                                    .strong()
                                    .background_color(ui.visuals().faint_bg_color),
                            );
                            ui.add_space(6.0);
                        }
                    });
                });
            });
        }

        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ui, |ui| {
                let area = ui.max_rect();
                ui.painter().rect_filled(area, 0.0, self.theme.background);
                self.grid = view::grid_size(area, cell);
                let a = self.active;
                // Where the panes and dividers go.
                let (mut placed, mut divs) = (vec![], vec![]);
                self.tabs[a].tree().layout(area, &mut placed, &mut divs);
                let placed: Vec<(u64, egui::Rect)> =
                    placed.into_iter().map(|(p, r)| (p.id, r)).collect();
                let multi = placed.len() > 1;
                let focus = self.tabs[a].pane().id;
                let window_focused = ctx.input(|i| i.focused);
                let mut focus_to = None;
                let mut click_browse = None;
                for &(id, rect) in &placed {
                    let response = ui.interact(
                        rect,
                        egui::Id::unique(("pane", id)),
                        Sense::click_and_drag(),
                    );
                    let focused = id == focus;
                    let Some(pane) = self.tabs[a].tree_mut().find_mut(&|p: &Pane| p.id == id)
                    else {
                        continue;
                    };
                    let grid = view::grid_size(rect, cell);
                    if grid != pane.grid {
                        pane.grid = grid;
                        pane.session.resize(grid.0, grid.1);
                    }
                    let mut screen = pane.session.screen.lock().unwrap();
                    let browse_look = pane.browse.as_ref().filter(|_| focused).map(|b| {
                        let top = screen.term.grid().history_size() as i64
                            - screen.term.grid().display_offset() as i64;
                        let bottom = screen.abs_cursor_line();
                        let block =
                            browse::block_at(&screen.commands.blocks, b.row, bottom).map(|i| {
                                let bl = &screen.commands.blocks[i];
                                (
                                    (bl.prompt - top) as i32,
                                    (browse::block_end(bl, bottom) - top) as i32,
                                    bl.exit.is_some_and(|e| e != 0),
                                )
                            });
                        view::BrowseLook {
                            row: (b.row - top) as i32,
                            col: b.col,
                            block,
                        }
                    });

                    // Mouse wheel: scroll back; in full-screen programs, arrow keys.
                    let scroll = ctx.input(|i| i.smooth_scroll_delta.y);
                    if scroll.abs() > 0.5 && response.hovered() {
                        let lines = (scroll / cell.y).round() as i32;
                        if lines != 0 {
                            let mode = *screen.term.mode();
                            if mode.contains(TermMode::ALT_SCREEN)
                                && mode.contains(TermMode::ALTERNATE_SCROLL)
                            {
                                let key = if lines > 0 { b"\x1bOA" } else { b"\x1bOB" };
                                for _ in 0..lines.unsigned_abs() {
                                    pane.session.write(key);
                                }
                            } else {
                                screen.term.scroll_display(Scroll::Delta(lines));
                            }
                        }
                    }

                    let blink_on = !self.settings.cursor_blink
                        || (self.started.elapsed().as_millis() / 530).is_multiple_of(2);
                    let look = Look {
                        theme: &self.theme,
                        font_size,
                        cell,
                        cursor_style: &self.settings.cursor_style,
                        focused: window_focused && focused,
                        blink_on,
                        browse: browse_look,
                    };
                    let painter = ui.painter_at(rect);
                    let origin = view::paint(&painter, rect, &screen, &look);

                    if let Some(pos) = response.interact_pointer_pos() {
                        let (point, side) = view::point_at(pos, origin, cell, &screen);
                        if !focused && (response.clicked() || response.drag_started()) {
                            // A click in another pane gives it the focus.
                            focus_to = Some(id);
                        }
                        if response.drag_started() {
                            screen.term.selection =
                                Some(Selection::new(SelectionType::Simple, point, side));
                            self.selecting = true;
                        } else if response.dragged() && self.selecting {
                            if let Some(sel) = screen.term.selection.as_mut() {
                                sel.update(point, side);
                            }
                        } else if response.clicked() && focused {
                            screen.term.selection = None;
                            // A click on an earlier line: browse mode there.
                            let top = screen.term.grid().history_size() as i64
                                - screen.term.grid().display_offset() as i64;
                            let row = top
                                + point.line.0 as i64
                                + screen.term.grid().display_offset() as i64;
                            if screen.prompt_seen
                                && row < screen.abs_cursor_line()
                                && !screen.term.mode().contains(TermMode::ALT_SCREEN)
                            {
                                click_browse = Some((id, row, point.column.0));
                            }
                        }
                    }
                    if response.drag_stopped() {
                        self.selecting = false;
                    }
                    if let Some(code) = screen.exited {
                        painter.text(
                            rect.center_bottom() - Vec2::new(0.0, 24.0),
                            egui::Align2::CENTER_CENTER,
                            format!("[the shell exited with code {code} — ⌘W to close]"),
                            view::mono(font_size * 0.9),
                            Color32::GRAY,
                        );
                    }
                    drop(screen);
                    if multi {
                        pane_marker(ui.painter(), rect, focused, &self.theme);
                    }
                }

                // Dividers: drag to resize, double-click for equal halves.
                let line = self.theme.foreground.gamma_multiply(0.25);
                for d in &divs {
                    let r = ui.interact(
                        d.rect.expand(2.0),
                        egui::Id::unique(("divider", d.path.clone())),
                        Sense::click_and_drag(),
                    );
                    let (icon, a1, a2) = match d.dir {
                        Dir::Right => (
                            egui::CursorIcon::ResizeHorizontal,
                            d.rect.center_top(),
                            d.rect.center_bottom(),
                        ),
                        Dir::Down => (
                            egui::CursorIcon::ResizeVertical,
                            d.rect.left_center(),
                            d.rect.right_center(),
                        ),
                    };
                    let active = r.hovered() || r.dragged();
                    if active {
                        ctx.set_cursor_icon(icon);
                    }
                    let stroke = if active { 2.0 } else { 1.0 };
                    ui.painter()
                        .line_segment([a1, a2], egui::Stroke::new(stroke, line));
                    if r.double_clicked() {
                        self.tabs[a].tree_mut().equalize(d);
                    } else if r.drag_started() {
                        self.dragging = Some(d.clone());
                    }
                    if r.dragged()
                        && let (Some(pos), Some(dd)) = (r.interact_pointer_pos(), &self.dragging)
                    {
                        let dd = dd.clone();
                        self.tabs[a].tree_mut().drag(&dd, pos);
                    }
                    if r.drag_stopped() {
                        self.dragging = None;
                    }
                }

                if let Some(id) = focus_to {
                    self.tabs[a].focus = id;
                }
                if let Some((id, row, col)) = click_browse
                    && let Some(p) = self.tabs[a].tree_mut().find_mut(&|p: &Pane| p.id == id)
                {
                    p.browse = Some(Browse::new(row, col));
                }
                self.pane_rects = placed;
            });

        #[cfg(target_os = "macos")]
        if let Some(m) = &self.menu {
            m.set_split(self.tabs[self.active].tree().count() > 1);
        }

        // The window title: "Home — zsh".
        let tab = &self.tabs[self.active];
        let title = tab_name(&tab.folder(&self.home), &tab.program(&self.shell_name));
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(title));
        if self.settings.cursor_blink {
            ctx.request_repaint_after(Duration::from_millis(530));
        }
        if let Some(smoke) = self.smoke.as_mut() {
            smoke.find_status = self.find.as_ref().map(|f| f.status());
            smoke.step(&ctx, &self.tabs, self.active);
        }
    }
}

/// Which pane has the focus, in shape as well as color: the focused pane
/// gets a frame and a notch in its top-left corner; the others are dimmed.
fn pane_marker(p: &egui::Painter, rect: egui::Rect, focused: bool, theme: &Theme) {
    if focused {
        let accent = theme.cursor;
        p.rect_stroke(
            rect.shrink(0.5),
            0.0,
            egui::Stroke::new(1.5, accent),
            egui::StrokeKind::Inside,
        );
        let n = 12.0;
        let c = rect.left_top();
        p.add(egui::Shape::convex_polygon(
            vec![c, c + Vec2::new(n, 0.0), c + Vec2::new(0.0, n)],
            accent,
            egui::Stroke::NONE,
        ));
    } else {
        p.rect_filled(rect, 0.0, theme.background.gamma_multiply(0.45));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_in_words() {
        assert_eq!(folder_name("/Users/me", "/Users/me"), "Home");
        assert_eq!(folder_name("/Users/me/code/app/", "/Users/me"), "app");
        assert_eq!(folder_name("/", "/Users/me"), "Computer root");
        assert_eq!(tab_name("Home", "zsh"), "Home — zsh");
        assert_eq!(program_of("sudo npm run build").as_deref(), Some("npm"));
        assert_eq!(
            program_of("FOO=1 /usr/bin/make -j4").as_deref(),
            Some("make")
        );
    }
}
