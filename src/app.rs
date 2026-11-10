//! The window: tabs, the tab bar, keyboard and mouse, and the terminal view.

use crate::alert::{self, Closing};
use crate::browse::{self, Action, Browse};
use crate::commands::{self, Cmd};
use crate::find::Find;
use crate::input::{is_app_shortcut, key_bytes, paste_bytes};
use crate::keys::Keys;
use crate::palette;
use crate::panes::{self, Dir, Node};
use crate::restore;
use crate::session::Session;
use crate::settings::{self, Settings};
use crate::shell::ShellSpec;
use crate::tabdrag;
use crate::themes::{self, Theme};
use crate::view::{self, Look};
use crate::{ai, aipanel};
use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::index::{Column, Line, Point, Side};
use alacritty_terminal::selection::{Selection, SelectionType};
use alacritty_terminal::term::TermMode;
use egui::{Color32, Key, RichText, Sense, Vec2};
use std::path::{Path, PathBuf};
use std::sync::{Arc, mpsc};
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
    /// Finished commands already noted (for notifications, the finish line
    /// and the stats).
    noted: usize,
    /// Smooth cursor: where it's drawn now, and where it really is.
    cursor_drawn: Option<egui::Pos2>,
    cursor_real: Option<egui::Pos2>,
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

/// What the app starts with, read before the window opens (the saved window
/// frame is needed for that).
pub struct Startup {
    settings_path: PathBuf,
    settings: Settings,
    session_path: PathBuf,
    /// The windows to bring back, the first window first (when restoring
    /// is on in Settings).
    pub restored: Vec<restore::SavedWindow>,
    /// How jamalterm quit last time, if it crashed (scrubbed).
    last_crash: Option<String>,
}

impl Startup {
    pub fn load() -> Startup {
        let settings_path = std::env::var_os("JAMALTERM_SETTINGS")
            .map(PathBuf::from)
            .unwrap_or_else(settings::default_path);
        let settings = settings::load(&settings_path);
        let session_path = settings_path.with_file_name("session.json");
        // The self-test starts fresh, except in its restore run.
        let smoke = std::env::var_os("JAMALTERM_SMOKE").is_some();
        let restored = if settings.restore_session && (!smoke || crate::smoke::restore_run()) {
            restore::load(&session_path)
        } else {
            vec![]
        };
        let dir = settings_path
            .parent()
            .unwrap_or(Path::new("."))
            .to_path_buf();
        crate::diag::init(&dir, settings.keep_problem_log);
        let last_crash = crate::diag::watch_crashes(&dir);
        Startup {
            settings_path,
            settings,
            session_path,
            restored,
            last_crash,
        }
    }
}

/// One window's state. The window being drawn is `App::w`; the others wait
/// in `App::others` and are swapped in to be drawn (and to run commands), so
/// everything that acts on "the window" acts on `self.w`.
pub struct Win {
    /// Its number (its viewport's id; 1 is the first window).
    pub id: u64,
    pub tabs: Vec<Tab>,
    pub active: usize,
    grid: (u16, u16),
    selecting: bool,
    /// The find bar (⌘F), when open.
    find: Option<Find>,
    focus_find: bool,
    /// Where each pane of the tab in front was last drawn (for ⌥⌘ arrows).
    pane_rects: Vec<(u64, egui::Rect)>,
    /// The divider being dragged.
    dragging: Option<panes::Divider>,
    /// Its close button was clicked (or ⇧⌘W).
    exiting: bool,
    /// Where it is on screen (saved with the session).
    frame: Option<restore::Frame>,
    /// Where it should open (a restored window's frame).
    open_at: Option<restore::Frame>,
    /// The command palette (⇧⌘P), when open.
    palette: Option<palette::Palette>,
    /// The finish line card ("✓ build done in 3m 12s") and until when.
    finish: Option<(String, Instant)>,
    /// The AI panel (⌘I) and its conversation.
    ai: aipanel::Chat,
    ai_open: bool,
    /// The tab being dragged (its place in the bar), and the pointer's last
    /// position in this window.
    tab_drag: Option<usize>,
    drag_pos: Option<egui::Pos2>,
    /// It's the app's first window (eframe's root viewport): closing it
    /// while others are open hands it another window's tabs instead.
    root: bool,
}

impl Win {
    fn new(id: u64) -> Win {
        Win {
            id,
            tabs: vec![],
            active: 0,
            grid: (80, 24),
            selecting: false,
            find: None,
            focus_find: false,
            pane_rects: vec![],
            dragging: None,
            exiting: false,
            frame: None,
            open_at: None,
            palette: None,
            finish: None,
            ai: aipanel::Chat::default(),
            ai_open: false,
            tab_drag: None,
            drag_pos: None,
            root: false,
        }
    }
}

pub struct App {
    /// The window being drawn (or in front, between frames: the first).
    w: Win,
    /// The other windows.
    others: Vec<Win>,
    /// The next window's id.
    next_win: u64,
    /// The window in front (its id), where menu commands go.
    front: u64,
    /// Menu commands waiting for the window in front to be drawn.
    pending: Vec<Cmd>,
    /// Quitting: every window closes, and the session keeps them all.
    quitting: bool,
    /// The Settings window (⌘,), open at this pane.
    settings_open: Option<palette::Pane>,
    /// Bring the Settings window to the front next frame.
    focus_settings: bool,
    /// The tab bar was hidden with View → Hide Tab Bar (even with tabs).
    tab_bar_hidden: bool,
    /// What was run from the palette lately (shown first).
    recent: Vec<palette::Action>,
    /// A "terminate running processes?" question waiting for its answer.
    asking: Option<alert::Pending>,
    /// The close that was just allowed (it goes ahead without asking again).
    allowed: Option<alert::Closing>,
    /// API keys (the encrypted key file).
    keys: Keys,
    /// The AI pane's state in Settings.
    prefs: crate::prefs::State,
    /// The self-test's stand-in for an AI provider: (address, key).
    ai_override: Option<(String, String)>,
    /// jamalterm crashed last time: the notice offers the diagnostics.
    last_crash: Option<String>,
    /// The diagnostics window, open with its report.
    diagnostics: Option<String>,
    /// Some window of ours is in front (this frame).
    app_active: bool,
    /// Your Day in jamalterm: program names and timings (stats.json).
    stats: crate::fun::Stats,
    stats_path: PathBuf,
    your_day: bool,
    /// The first-start card (shown once).
    welcome: bool,
    #[cfg(target_os = "macos")]
    switcher: crate::menu::Switcher,
    /// The next pane's id.
    next_id: u64,
    settings: Settings,
    settings_path: PathBuf,
    /// session.json, next to settings.json.
    session_path: PathBuf,
    /// What was last written there (it's only written when it changes).
    last_saved: String,
    saved_at: Instant,
    theme: Theme,
    home: String,
    shell_name: String,
    started: Instant,
    smoke: Option<crate::smoke::Smoke>,
    ctx: egui::Context,
    #[cfg(target_os = "macos")]
    menu: Option<crate::menu::MacMenu>,
}

impl App {
    pub fn new(cc: &eframe::CreationContext, start: Startup) -> Self {
        crate::fonts::install(&cc.egui_ctx);
        let Startup {
            settings_path,
            settings,
            session_path,
            restored,
            last_crash,
        } = start;
        let theme = themes::by_id(&settings.theme);
        let shell = if settings.shell.is_empty() {
            crate::shell::default_shell()
        } else {
            settings.shell.clone()
        };
        let keys = Keys::open(&settings_path);
        let dir = settings_path
            .parent()
            .unwrap_or(Path::new("."))
            .to_path_buf();
        let stats_path = dir.join("stats.json");
        // The first-start card: once, and never when windows were reopened.
        let welcomed = dir.join(".welcomed");
        let welcome = !welcomed.exists()
            && restored.is_empty()
            && std::env::var_os("JAMALTERM_SMOKE").is_none();
        if welcome {
            let _ = std::fs::create_dir_all(&dir);
            let _ = std::fs::write(&welcomed, b"");
        }
        let smoke = crate::smoke::Smoke::from_env();
        let ai_override = smoke
            .as_ref()
            .map(|s| (s.ai_server.clone(), "sk-ant-smoke".to_string()));
        let mut app = App {
            w: Win {
                root: true,
                ..Win::new(1)
            },
            others: vec![],
            next_win: 2,
            front: 1,
            pending: vec![],
            quitting: false,
            settings_open: None,
            focus_settings: false,
            tab_bar_hidden: false,
            recent: vec![],
            asking: None,
            allowed: None,
            keys,
            prefs: Default::default(),
            ai_override,
            last_crash,
            diagnostics: None,
            app_active: false,
            stats: crate::fun::Stats::load(&stats_path),
            stats_path,
            your_day: false,
            welcome,
            #[cfg(target_os = "macos")]
            switcher: crate::menu::Switcher::new(),
            next_id: 1,
            theme,
            home: dirs::home_dir()
                .map(|h| h.display().to_string())
                .unwrap_or_default(),
            shell_name: shell.rsplit('/').next().unwrap_or("shell").to_string(),
            settings,
            settings_path,
            session_path,
            last_saved: String::new(),
            saved_at: Instant::now(),
            started: Instant::now(),
            smoke,
            ctx: cc.egui_ctx.clone(),
            #[cfg(target_os = "macos")]
            menu: crate::menu::MacMenu::install(&cc.egui_ctx),
        };
        #[cfg(target_os = "macos")]
        if let Some(s) = app.smoke.as_mut() {
            s.menu_bar = Some(app.menu.is_some());
        }
        let mut saved = restored.into_iter();
        if let Some(w) = saved.next() {
            app.restore(&cc.egui_ctx, &w);
        }
        for sw in saved {
            let mut w = Win::new(app.next_win);
            app.next_win += 1;
            w.open_at = sw.frame;
            std::mem::swap(&mut app.w, &mut w);
            app.restore(&cc.egui_ctx, &sw);
            std::mem::swap(&mut app.w, &mut w);
            if !w.tabs.is_empty() {
                app.others.push(w);
            }
        }
        if app.w.tabs.is_empty() {
            app.new_tab(&cc.egui_ctx, None);
        }
        app
    }

    /// Brings back a saved window's tabs and splits, each pane in its
    /// folder with its recent output.
    fn restore(&mut self, ctx: &egui::Context, w: &restore::SavedWindow) {
        for g in w.groups() {
            let mut focus = None;
            let node = restore::to_node(&g).filter_map(&mut |t: restore::SavedTab| {
                let dir = (!t.cwd.is_empty() && Path::new(&t.cwd).is_dir())
                    .then(|| PathBuf::from(&t.cwd));
                let mut preface = t.output.clone().into_bytes();
                if !preface.is_empty() {
                    preface.extend_from_slice(
                        b"\r\n\x1b[2m[Restored from the last session]\x1b[0m\r\n",
                    );
                }
                let p = self.spawn_pane(ctx, dir, &preface)?;
                if t.focused {
                    focus = Some(p.id);
                }
                Some(p)
            });
            if let Some(root) = node {
                let first = root.leaves()[0].id;
                self.w.tabs.push(Tab {
                    root: Some(root),
                    focus: focus.unwrap_or(first),
                });
            }
        }
        self.w.active = w.active.min(self.w.tabs.len().saturating_sub(1));
    }

    /// A window as it's saved: tabs, splits, folders, recent output, frame.
    fn snapshot(w: &Win) -> restore::SavedWindow {
        restore::SavedWindow {
            active: w.active,
            groups: w
                .tabs
                .iter()
                .map(|t| save_node(t.tree(), t.focus))
                .collect(),
            tabs: vec![],
            frame: w.frame,
        }
    }

    /// Writes session.json if anything changed. A window that was closed
    /// isn't in it; with none left the file is emptied (a fresh start).
    fn save_session(&mut self, ctx: &egui::Context) {
        self.saved_at = Instant::now();
        if self.smoke.is_some() && !self.smoke.as_ref().is_some_and(|s| s.save_session) {
            return;
        }
        let _ = ctx;
        // Every window with tabs, the first window first.
        let mut all: Vec<&Win> = std::iter::once(&self.w).chain(&self.others).collect();
        all.sort_by_key(|w| (!w.root, w.id));
        let windows: Vec<restore::SavedWindow> = all
            .into_iter()
            .filter(|w| !w.tabs.is_empty() && !w.exiting)
            .map(App::snapshot)
            .collect();
        let data = restore::encode(&windows);
        if data != self.last_saved {
            match restore::save(&self.session_path, &data) {
                Ok(()) => self.last_saved = data,
                Err(e) => {
                    crate::diag::problem("session", &format!("the session wasn't saved: {e}"))
                }
            }
        }
    }

    /// Starts a shell for a new tab or pane, in the focused pane's folder
    /// (like Terminal), or in `dir`.
    fn spawn_pane(
        &mut self,
        ctx: &egui::Context,
        dir: Option<PathBuf>,
        preface: &[u8],
    ) -> Option<Pane> {
        let (dir, grid) = match self.pane() {
            Some(p) => {
                let cwd = p.session.screen.lock().unwrap().cwd.clone();
                (
                    dir.or_else(|| (!cwd.is_empty()).then(|| PathBuf::from(cwd))),
                    p.grid,
                )
            }
            None => (dir, self.w.grid),
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
        match Session::spawn(
            &spec,
            grid.0,
            grid.1,
            self.settings.scrollback,
            preface,
            repaint,
        ) {
            Ok(session) => {
                self.next_id += 1;
                Some(Pane {
                    id: self.next_id,
                    session,
                    grid,
                    browse: None,
                    typed: false,
                    prompts: 0,
                    noted: 0,
                    cursor_drawn: None,
                    cursor_real: None,
                })
            }
            Err(e) => {
                crate::diag::problem("shell", &format!("the shell didn't start: {e}"));
                None
            }
        }
    }

    fn new_tab(&mut self, ctx: &egui::Context, dir: Option<PathBuf>) {
        if let Some(p) = self.spawn_pane(ctx, dir, b"") {
            self.w.tabs.push(Tab::new(p));
            self.w.active = self.w.tabs.len() - 1;
        }
    }

    /// Splits the focused pane (⌘D right, ⇧⌘D down).
    fn split(&mut self, ctx: &egui::Context, dir: Dir) {
        if let Some(p) = self.spawn_pane(ctx, None, b"")
            && let Some(t) = self.w.tabs.get_mut(self.w.active)
        {
            t.split(dir, p);
        }
    }

    /// ⌘W: closes the focused pane, or the tab when it isn't split.
    fn close_pane(&mut self, ctx: &egui::Context) {
        let Some(t) = self.w.tabs.get(self.w.active) else {
            return;
        };
        let id = t.pane().id;
        if t.tree().count() == 1 {
            self.close_tab(ctx, self.w.active);
            return;
        }
        if !self.may_close(
            ctx,
            Closing::Pane {
                win: self.w.id,
                pane: id,
            },
        ) {
            return;
        }
        if let Some(t) = self.w.tabs.get_mut(self.w.active) {
            t.close_pane(id);
        }
    }

    /// The focused pane of the tab in front.
    fn pane(&self) -> Option<&Pane> {
        self.w.tabs.get(self.w.active).map(|t| t.pane())
    }

    fn pane_mut(&mut self) -> Option<&mut Pane> {
        self.w.tabs.get_mut(self.w.active).map(|t| t.pane_mut())
    }

    /// Moves the focus to the next/previous pane (⌘] ⌘[) or the one in a
    /// direction (⌥⌘ arrows), using where the panes were last drawn.
    fn focus_pane(&mut self, step: i32, toward: Option<Vec2>) {
        let Some(t) = self.w.tabs.get_mut(self.w.active) else {
            return;
        };
        let ids: Vec<u64> = t.panes().iter().map(|p| p.id).collect();
        let Some(i) = ids.iter().position(|id| *id == t.focus) else {
            return;
        };
        let next = match toward {
            Some(v) => {
                let rects: Vec<egui::Rect> = self.w.pane_rects.iter().map(|(_, r)| *r).collect();
                let at = self.w.pane_rects.iter().position(|(id, _)| *id == t.focus);
                at.and_then(|at| panes::neighbour(&rects, at, v))
                    .map(|j| self.w.pane_rects[j].0)
            }
            None => Some(ids[(i as i64 + step as i64).rem_euclid(ids.len() as i64) as usize]),
        };
        if let Some(n) = next {
            t.focus = n;
        }
    }

    fn close_tab(&mut self, ctx: &egui::Context, i: usize) {
        let Some(t) = self.w.tabs.get(i) else {
            return;
        };
        let pane = t.panes()[0].id;
        if !self.may_close(
            ctx,
            Closing::Tab {
                win: self.w.id,
                pane,
            },
        ) {
            return;
        }
        self.w.tabs.remove(i); // dropping the session ends its shell
        if self.w.tabs.is_empty() {
            // The window goes with its last tab (and is forgotten). The last
            // window going quits, and the next start is fresh.
            if self.w.root && self.others.is_empty() {
                self.save_session(ctx);
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            return;
        }
        self.w.active = self.w.active.min(self.w.tabs.len() - 1);
    }

    /// ⌘N: a new window with one tab, in the home folder (as Terminal).
    fn new_window(&mut self, ctx: &egui::Context) {
        let mut w = Win::new(self.next_win);
        self.next_win += 1;
        std::mem::swap(&mut self.w, &mut w);
        self.new_tab(ctx, None);
        std::mem::swap(&mut self.w, &mut w);
        if !w.tabs.is_empty() {
            self.front = w.id;
            self.others.push(w);
        }
    }

    /// A new window holding these tabs.
    fn window_with(&mut self, tabs: Vec<Tab>) {
        let mut w = Win::new(self.next_win);
        self.next_win += 1;
        w.tabs = tabs;
        self.front = w.id;
        self.others.push(w);
    }

    /// Window → Move Tab to New Window (off with only one tab).
    fn move_tab_to_new_window(&mut self) {
        if self.w.tabs.len() < 2 {
            return;
        }
        let t = self.w.tabs.remove(self.w.active);
        self.w.active = self.w.active.min(self.w.tabs.len() - 1);
        self.window_with(vec![t]);
    }

    /// Window → Merge All Windows: every tab into this window.
    fn merge_all_windows(&mut self) {
        for w in &mut self.others {
            self.w.tabs.append(&mut w.tabs);
        }
        // The emptied windows close at the end of the frame.
    }

    /// The Settings window, while open (its own native window).
    fn settings_window(&mut self, ctx: &egui::Context) {
        let Some(mut pane) = self.settings_open else {
            return;
        };
        let vp = egui::ViewportId::from_hash_of("jamalterm-settings");
        if std::mem::take(&mut self.focus_settings) {
            ctx.send_viewport_cmd_to(vp, egui::ViewportCommand::Focus);
        }
        // ⌘W (a menu key equivalent) closes it when it's in front.
        if self.front == SETTINGS {
            let cmds = std::mem::take(&mut self.pending);
            if cmds
                .iter()
                .any(|c| matches!(c, Cmd::ClosePane | Cmd::CloseTab | Cmd::CloseWindow))
            {
                self.close_settings();
                return;
            }
            self.pending = cmds; // anything else: for the terminal window
            self.front = self.w.id;
        }
        let builder = egui::ViewportBuilder::default()
            .with_title(pane.name()) // the window title names the pane
            .with_inner_size([560.0, 520.0])
            .with_min_inner_size([420.0, 320.0]);
        let mut settings = self.settings.clone();
        let mut out = None;
        let mut close = false;
        let mut focused = false;
        ctx.show_viewport_immediate(vp, builder, |ui, _| {
            let ctx = ui.ctx().clone();
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(pane.name().into()));
            ctx.input(|i| {
                close = i.viewport().close_requested()
                    || i.key_pressed(Key::Escape)
                    || (i.modifiers.command && i.key_pressed(Key::W));
                focused = i.viewport().focused == Some(true);
            });
            egui::CentralPanel::default().show(ui, |ui| {
                out = Some(crate::prefs::show(
                    ui,
                    &mut settings,
                    &mut pane,
                    &mut self.keys,
                    &mut self.prefs,
                ));
            });
        });
        if focused {
            self.front = SETTINGS;
        }
        self.settings_open = Some(pane);
        if let Some(out) = out {
            if out.changed {
                self.settings = settings;
                self.settings_changed();
            }
            // Hovering a theme shows it everywhere; moving away puts yours back.
            let want = out.preview.unwrap_or(self.settings.theme.as_str());
            if self.theme.id != want {
                self.theme = themes::by_id(want);
            }
        }
        if close {
            self.close_settings();
        }
    }

    fn close_settings(&mut self) {
        self.settings_open = None;
        self.theme = themes::by_id(&self.settings.theme);
        if self.front == SETTINGS {
            self.front = self.w.id;
        }
    }

    /// Where AI questions go now (or why they can't go).
    fn ai_endpoint(&self) -> Result<ai::Endpoint, String> {
        if let Some((base, key)) = &self.ai_override {
            return Ok(ai::Endpoint {
                provider: "anthropic",
                base: base.clone(),
                key: key.clone(),
                model: String::new(),
            });
        }
        ai::Endpoint::from_settings(&self.settings, self.keys.get(&self.settings.ai_provider))
    }

    /// Recent commands, exit codes and (unless switched off) output in the
    /// pane with the focus, and the folder: what goes with a question.
    fn ai_context(&self) -> String {
        let Some(p) = self.pane() else {
            return String::new();
        };
        let s = p.session.screen.lock().unwrap();
        let bottom = s.abs_cursor_line();
        let mut parts = vec![];
        let mut budget = 8000usize;
        for b in s.commands.blocks.iter().rev().take(10) {
            let mut t = format!("$ {}", b.command);
            if self.settings.ai_share_output {
                let from = b.start.unwrap_or(b.prompt + 1);
                let to = browse::block_end(b, bottom);
                let lines: Vec<String> = (from..=to).map(|l| s.line_text(l)).collect();
                let keep = lines.len().saturating_sub(60);
                let out = lines[keep..].join("\n");
                if !out.trim().is_empty() {
                    t.push('\n');
                    t.push_str(out.trim_end());
                }
            }
            if let Some(code) = b.exit {
                t.push_str(&format!("\n(exit {code})"));
            }
            if t.len() > budget {
                break;
            }
            budget -= t.len();
            parts.push(t);
        }
        parts.reverse();
        let mut c = parts.join("\n\n");
        if !s.cwd.is_empty() {
            c = format!("Folder: {}\n\n{c}", s.cwd);
        }
        c
    }

    /// Sends a question from the AI panel.
    fn ai_ask(&mut self, ctx: &egui::Context, prompt: String) {
        let ep = match self.ai_endpoint() {
            Ok(ep) => ep,
            Err(e) => {
                self.w.ai.items.push(aipanel::Item::Error(e));
                return;
            }
        };
        let context = self.ai_context();
        self.w.ai.items.push(aipanel::Item::User(prompt.clone()));
        self.w.ai.shown = context.clone();
        let ctx2 = ctx.clone();
        self.w.ai.running = Some(ai::ask(
            ep,
            self.w.ai.history.clone(),
            ai::Question {
                prompt,
                context,
                shell: self.shell_name.clone(),
            },
            Arc::new(move || ctx2.request_repaint()),
        ));
    }

    /// Types a command at the prompt of the pane with the focus (and runs
    /// it). Returns that pane and how many commands it had.
    fn type_at_prompt(&mut self, command: &str, run: bool) -> Option<(u64, usize)> {
        let vi = self.settings.prompt_keys == "vi";
        let p = self.pane_mut()?;
        p.browse = None;
        let mut s = p.session.screen.lock().unwrap();
        s.term.scroll_display(Scroll::Bottom);
        let before = s.commands.blocks.len();
        drop(s);
        p.session
            .write(&browse::type_command(command, None, run, vi));
        p.typed = !run;
        Some((p.id, before))
    }

    /// The AI panel's events, tool calls and the command it's waiting on.
    fn ai_tick(&mut self) {
        let errors_before = self
            .w
            .ai
            .items
            .iter()
            .filter(|i| matches!(i, aipanel::Item::Error(_)))
            .count();
        let tools = self.w.ai.poll();
        // AI errors go in the problem log: the message, never the question.
        for i in self
            .w
            .ai
            .items
            .iter()
            .filter_map(|i| match i {
                aipanel::Item::Error(e) => Some(e),
                _ => None,
            })
            .skip(errors_before)
        {
            crate::diag::problem("ai", i);
        }
        for (command, run) in tools {
            if run {
                let risk = ai::risk::check(&command, &self.w.ai.shown);
                self.w.ai.items.push(aipanel::Item::Tool(aipanel::Card {
                    command,
                    risk,
                    armed: false,
                    state: aipanel::CardState::Asking,
                }));
            } else {
                let typed = self.type_at_prompt(&command, false).is_some();
                self.w.ai.items.push(aipanel::Item::Note(format!(
                    "Typed at the prompt: {command}"
                )));
                self.w.ai.reply(
                    if typed {
                        "Typed the command at the prompt; the user will press Return to run it."
                            .into()
                    } else {
                        "There's no terminal to type into.".into()
                    },
                    !typed,
                );
            }
        }
        // A command the AI ran: tell it how it went once it's finished.
        let Some(card) = self.w.ai.card_mut() else {
            return;
        };
        let aipanel::CardState::Running {
            pane,
            before,
            since,
        } = card.state.clone()
        else {
            return;
        };
        let found = self
            .w
            .tabs
            .iter()
            .flat_map(|t| t.panes())
            .find(|p| p.id == pane)
            .map(|p| {
                let s = p.session.screen.lock().unwrap();
                s.commands.blocks.get(before..).and_then(|new| {
                    new.iter().rev().find(|b| b.exit.is_some()).map(|b| {
                        let bottom = s.abs_cursor_line();
                        let from = b.start.unwrap_or(b.prompt + 1);
                        let to = browse::block_end(b, bottom);
                        let lines: Vec<String> = (from..=to).map(|l| s.line_text(l)).collect();
                        let keep = lines.len().saturating_sub(40);
                        (
                            b.exit.unwrap_or(0),
                            lines[keep..].join("\n").trim_end().to_string(),
                        )
                    })
                })
            });
        let (text, state) = match found {
            None => (
                ("The tab it was running in was closed.".to_string(), true),
                "The tab was closed.".to_string(),
            ),
            Some(Some((code, out))) => {
                let (out, _) = ai::redact::redact(&out);
                (
                    (format!("exit code {code}\n{out}"), false),
                    format!(
                        "{} Ran it: exit code {code}",
                        if code == 0 { "✓" } else { "✕" }
                    ),
                )
            }
            Some(None) if since.elapsed() > Duration::from_secs(120) => (
                (
                    "Still running after 2 minutes; the user can see it in the terminal."
                        .to_string(),
                    false,
                ),
                "… still running (the AI was told)".to_string(),
            ),
            Some(None) => return,
        };
        if let Some(card) = self.w.ai.card_mut() {
            card.state = aipanel::CardState::Done(state);
        }
        self.w.ai.reply(text.0, text.1);
    }

    /// Carries out what was done in the AI panel.
    fn ai_act(&mut self, ctx: &egui::Context, act: aipanel::Act) {
        use aipanel::Act;
        match act {
            Act::Ask(q) => self.ai_ask(ctx, q),
            Act::Type { command, run } => {
                self.type_at_prompt(&command, run);
            }
            Act::Decide(run) => {
                let Some(command) = self.w.ai.card_mut().map(|c| c.command.clone()) else {
                    return;
                };
                if run {
                    let at = self.type_at_prompt(&command, true);
                    if let Some(card) = self.w.ai.card_mut() {
                        card.state = match at {
                            Some((pane, before)) => aipanel::CardState::Running {
                                pane,
                                before,
                                since: Instant::now(),
                            },
                            None => aipanel::CardState::Done("No terminal to run it in.".into()),
                        };
                    }
                    if at.is_none() {
                        self.w
                            .ai
                            .reply("There's no terminal to run it in.".into(), true);
                    }
                } else {
                    if let Some(card) = self.w.ai.card_mut() {
                        card.state = aipanel::CardState::Done("Not run.".into());
                    }
                    self.w
                        .ai
                        .reply("The user chose not to run it.".into(), true);
                }
            }
            Act::Stop => {
                if let Some(r) = self.w.ai.running.take() {
                    r.cancel.store(true, std::sync::atomic::Ordering::Relaxed);
                    let _ = r.replies.send(ai::Reply {
                        text: "Stopped by the user.".into(),
                        is_error: true,
                    });
                }
                if let Some(card) = self.w.ai.card_mut() {
                    card.state = aipanel::CardState::Done("Stopped.".into());
                }
                self.w.ai.items.push(aipanel::Item::Note("Stopped.".into()));
            }
            Act::Close => self.w.ai_open = false,
            Act::Settings => self.open_settings(Some(palette::Pane::Ai)),
        }
    }

    /// The AI panel beside the terminal.
    fn ai_panel(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        self.ai_tick();
        if !self.w.ai_open {
            return;
        }
        let last_failed = self.pane().and_then(|p| {
            let s = p.session.screen.lock().unwrap();
            s.commands
                .last_finished()
                .filter(|b| b.exit.is_some_and(|e| e != 0) && !b.command.is_empty())
                .map(|b| b.command.clone())
        });
        let ready = self.ai_endpoint().map(|_| ());
        let provider = if self.ai_override.is_some() {
            "anthropic"
        } else {
            ai::provider(&self.settings.ai_provider).id
        };
        let fill = aipanel::tint(self.theme.background, self.theme.light);
        let frame = egui::Frame::new()
            .fill(fill)
            .inner_margin(10.0)
            .stroke(egui::Stroke::new(1.0, fill.gamma_multiply(1.6)));
        let mut acts = vec![];
        egui::Panel::right("ai")
            .frame(frame)
            .default_size(380.0)
            .min_size(260.0)
            .resizable(true)
            .show(ui, |ui| {
                let look = aipanel::Look {
                    provider,
                    ready,
                    enabled: self.settings.ai_enabled,
                    share_output: self.settings.ai_share_output,
                    last_failed: last_failed.as_deref(),
                };
                acts = aipanel::show(ui, &mut self.w.ai, &look);
            });
        for a in acts {
            self.ai_act(ctx, a);
        }
    }

    /// Secure Keyboard Entry: on while jamalterm is in front and either
    /// chosen in the menu or the pane in front is at a password prompt.
    fn secure_input(&mut self, was_active: bool) {
        let active = self.app_active || (was_active && self.settings_open.is_some());
        let at_password = self
            .pane()
            .is_some_and(|p| crate::secure::password_prompt(p.session.fd()));
        crate::secure::set(active && (self.settings.secure_input || at_password));
        #[cfg(target_os = "macos")]
        if let Some(m) = &self.menu {
            m.set_secure(self.settings.secure_input);
        }
        if let Some(s) = self.smoke.as_mut() {
            s.password_prompt = at_password;
        }
    }

    /// Help → Copy Diagnostics: version, system, settings (no keys, no
    /// folders) and the recent problem log, all scrubbed. Shown in full;
    /// copied only when you press Copy; never sent anywhere.
    fn diagnostics_report(&self) -> String {
        let s = &self.settings;
        let on = |b: bool| if b { "on" } else { "off" };
        let mut r = String::new();
        let mut line = |t: String| {
            r.push_str(&t);
            r.push('\n');
        };
        line("jamalterm diagnostics".into());
        line("(made on this computer and not sent anywhere; read it before sharing)".into());
        line(String::new());
        line(format!("Version:   {}", env!("CARGO_PKG_VERSION")));
        line(format!(
            "System:    {} ({})",
            os_version(),
            std::env::consts::ARCH
        ));
        line(format!(
            "Shell:     {}, integration {}",
            self.shell_name,
            on(s.shell_integration)
        ));
        let tabs: usize = std::iter::once(&self.w)
            .chain(&self.others)
            .map(|w| w.tabs.len())
            .sum();
        line(format!("Windows:   {}, tabs {tabs}", 1 + self.others.len()));
        let p = ai::provider(&s.ai_provider);
        let key = if p.local || self.keys.get(p.id).is_some() {
            "ready (key found; not shown)"
        } else {
            "no key"
        };
        line(format!(
            "AI:        {}, {}, {key}",
            on(s.ai_enabled),
            p.name
        ));
        line(format!(
            "Secure Keyboard Entry: {}{}",
            on(crate::secure::on()),
            if s.secure_input { " (always)" } else { "" }
        ));
        line(String::new());
        line("Settings:".into());
        for (k, v) in [
            ("theme", s.theme.clone()),
            ("font size", s.font_size.to_string()),
            ("line height", s.line_height.to_string()),
            ("cursor", s.cursor_style.clone()),
            ("cursor blink", s.cursor_blink.to_string()),
            ("scrollback", s.scrollback.to_string()),
            ("custom shell", (!s.shell.is_empty()).to_string()),
            ("↑ browses", s.arrow_browse.to_string()),
            ("restore windows", s.restore_session.to_string()),
            ("prompt keys", s.prompt_keys.clone()),
            ("confirm re-run", s.confirm_rerun.to_string()),
            ("option is meta", s.option_is_meta.to_string()),
            ("share output with AI", s.ai_share_output.to_string()),
            ("auto update", s.auto_update.to_string()),
            ("update channel", s.update_channel.clone()),
        ] {
            line(format!("  {k:<22} {v}"));
        }
        if let Some(c) = &self.last_crash {
            line(String::new());
            line("Last crash (scrubbed):".into());
            for l in c.lines() {
                line(format!("  {l}"));
            }
        }
        line(String::new());
        if !crate::diag::enabled() {
            line("Problem log: off (Settings → Privacy)".into());
        } else {
            let recent = crate::diag::recent(40);
            if recent.is_empty() {
                line("Recent problems: none".into());
            } else {
                line("Recent problems (oldest first, times in UTC):".into());
                for l in recent {
                    line(format!("  {l}"));
                }
            }
        }
        crate::diag::scrub_text(&r)
    }

    /// The diagnostics window: the whole report, and Copy.
    fn diagnostics_window(&mut self, ctx: &egui::Context) {
        let Some(mut text) = self.diagnostics.take() else {
            return;
        };
        let vp = egui::ViewportId::from_hash_of("jamalterm-diagnostics");
        let builder = egui::ViewportBuilder::default()
            .with_title("Diagnostics")
            .with_inner_size([620.0, 560.0]);
        let mut close = false;
        ctx.show_viewport_immediate(vp, builder, |ui, _| {
            let ctx = ui.ctx().clone();
            ctx.input(|i| {
                close = i.viewport().close_requested() || i.key_pressed(Key::Escape);
            });
            egui::Panel::bottom("diag-buttons").show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Nothing has been copied or sent. Read it, then copy it if you want to share it.");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Close").clicked() {
                            close = true;
                        }
                        if ui.button("Copy").clicked() {
                            ctx.copy_text(text.clone());
                        }
                    });
                });
            });
            egui::CentralPanel::default().show(ui, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    let mut shown = text.as_str(); // read-only
                    ui.add(
                        egui::TextEdit::multiline(&mut shown)
                            .font(egui::TextStyle::Monospace)
                            .desired_width(f32::INFINITY),
                    );
                });
            });
        });
        if let Some(s) = self.smoke.as_mut() {
            s.diagnostics = Some(std::mem::take(&mut text));
            self.diagnostics = None;
            return;
        }
        if !close {
            self.diagnostics = Some(text);
        }
    }

    /// "jamalterm quit unexpectedly last time", in the first window.
    fn crash_notice(&mut self, ui: &mut egui::Ui) {
        if !self.w.root || self.last_crash.is_none() {
            return;
        }
        let mut dismiss = false;
        let mut show = false;
        egui::Panel::top("crash-notice").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label("⚠ jamalterm quit unexpectedly last time. A report stayed on this Mac; nothing was sent.");
                if ui.button("Copy Diagnostics…").clicked() {
                    show = true;
                }
                if ui.button("Dismiss").clicked() {
                    dismiss = true;
                }
            });
        });
        if show {
            self.diagnostics = Some(self.diagnostics_report());
        }
        if dismiss {
            self.last_crash = None;
        }
    }

    /// Long commands that finished: a notification when you're in another
    /// app, the finish line when you watched; every finished command's
    /// program name and time go in Your Day's stats.
    fn finished_commands(&mut self) {
        let active = self.app_active;
        let front = self.front;
        let notify_after = self.settings.notify_after_seconds as u64;
        let mut stats_changed = false;
        let mut chime = false;
        let mut wins: Vec<&mut Win> = std::iter::once(&mut self.w)
            .chain(self.others.iter_mut())
            .collect();
        for w in wins.iter_mut() {
            let visible: Vec<u64> = w
                .tabs
                .get(w.active)
                .map(|t| t.panes().iter().map(|p| p.id).collect())
                .unwrap_or_default();
            let mut card = None;
            for t in &mut w.tabs {
                for p in t.tree_mut().leaves_mut() {
                    let s = p.session.screen.lock().unwrap();
                    let done: Vec<(String, i32, u64)> = s
                        .commands
                        .blocks
                        .iter()
                        .filter(|b| b.exit.is_some() && !b.command.is_empty())
                        .skip(p.noted)
                        .map(|b| {
                            let secs = match (b.started_at, b.finished_at) {
                                (Some(a), Some(z)) => z.duration_since(a).as_secs(),
                                _ => 0,
                            };
                            (b.command.clone(), b.exit.unwrap_or(0), secs)
                        })
                        .collect();
                    let total = s
                        .commands
                        .blocks
                        .iter()
                        .filter(|b| b.exit.is_some() && !b.command.is_empty())
                        .count();
                    drop(s);
                    if total < p.noted {
                        p.noted = total; // ⌘K cleared them
                    }
                    p.noted += done.len();
                    for (command, exit, secs) in done {
                        let program = program_of(&command).unwrap_or_else(|| "a command".into());
                        self.stats
                            .record(&program, secs, exit == 0, crate::fun::now_secs());
                        stats_changed = true;
                        if self.settings.notify_long_commands && secs >= notify_after && !active {
                            let (title, body) = crate::fun::notification(&command, exit, secs);
                            crate::macos::notify(&title, &body);
                        } else if self.settings.finish_moments
                            && secs >= 60
                            && active
                            && front == w.id
                            && visible.contains(&p.id)
                        {
                            card = Some(crate::fun::finish_line(&program, exit == 0, secs));
                            chime |= self.settings.finish_sound;
                        }
                    }
                }
            }
            if let Some(c) = card {
                w.finish = Some((c, Instant::now() + Duration::from_secs(5)));
            }
        }
        if chime {
            crate::macos::chime();
        }
        if stats_changed && self.smoke.as_ref().is_none_or(|s| s.save_session) {
            self.stats.save(&self.stats_path);
        }
    }

    /// The menu bar icon's and the Dock menu's list of windows and tabs.
    #[cfg(target_os = "macos")]
    fn update_switcher(&mut self) {
        let mut wins: Vec<&Win> = std::iter::once(&self.w).chain(&self.others).collect();
        wins.sort_by_key(|w| (!w.root, w.id));
        let list: Vec<crate::menu::Listed> = wins
            .iter()
            .filter(|w| !w.tabs.is_empty())
            .map(|w| {
                let tabs: Vec<String> = w
                    .tabs
                    .iter()
                    .map(|t| tab_name(&t.folder(&self.home), &t.program(&self.shell_name)))
                    .collect();
                crate::menu::Listed {
                    id: w.id,
                    name: tabs.get(w.active).cloned().unwrap_or_default(),
                    tabs,
                    active: w.active,
                }
            })
            .collect();
        self.switcher.update(&list, self.settings.menu_bar_icon);
    }

    /// Help → Your Day in jamalterm.
    fn your_day_window(&mut self, ctx: &egui::Context) {
        if !self.your_day {
            return;
        }
        let offset = local_utc_offset();
        let day = self.stats.day(crate::fun::now_secs(), offset);
        let vp = egui::ViewportId::from_hash_of("jamalterm-your-day");
        let builder = egui::ViewportBuilder::default()
            .with_title("Your Day in jamalterm")
            .with_inner_size([380.0, 340.0]);
        let (mut close, mut reset) = (false, false);
        ctx.show_viewport_immediate(vp, builder, |ui, _| {
            let ctx = ui.ctx().clone();
            ctx.input(|i| close = i.viewport().close_requested() || i.key_pressed(Key::Escape));
            egui::CentralPanel::default().show(ui, |ui| {
                ui.heading("Today");
                ui.label(format!("{} commands, {} failed", day.commands, day.failures));
                if let Some((p, secs)) = &day.longest {
                    ui.label(format!("Longest: {p}, {}", crate::fun::duration(*secs)));
                }
                ui.add_space(8.0);
                ui.heading("Most used this week");
                if day.week.is_empty() {
                    ui.weak("Nothing yet.");
                }
                for (p, n) in &day.week {
                    ui.label(format!("{p}: {n}"));
                }
                ui.add_space(8.0);
                ui.weak("Only program names and timings are kept, on this Mac, for a week; never arguments or output.");
                ui.horizontal(|ui| {
                    if ui.button("Reset").clicked() {
                        reset = true;
                    }
                    if ui.button("Close").clicked() {
                        close = true;
                    }
                });
            });
        });
        if reset {
            self.stats = Default::default();
            self.stats.save(&self.stats_path);
        }
        if close {
            self.your_day = false;
        }
    }

    /// The finish line card and the first-start card, bottom right.
    fn cards(&mut self, ctx: &egui::Context) {
        if let Some((text, until)) = &self.w.finish {
            if Instant::now() > *until {
                self.w.finish = None;
            } else {
                let text = text.clone();
                egui::Area::new(egui::Id::unique(("finish", self.w.id)))
                    .anchor(egui::Align2::RIGHT_BOTTOM, Vec2::new(-16.0, -16.0))
                    .order(egui::Order::Foreground)
                    .show(ctx, |ui| {
                        egui::Frame::popup(ui.style()).show(ui, |ui| {
                            ui.label(RichText::new(text).size(15.0));
                        });
                    });
                ctx.request_repaint_after(Duration::from_millis(500));
            }
        }
        if self.welcome && self.w.root {
            let mut done = false;
            egui::Area::new(egui::Id::unique("welcome"))
                .anchor(egui::Align2::RIGHT_BOTTOM, Vec2::new(-16.0, -16.0))
                .order(egui::Order::Foreground)
                .show(ctx, |ui| {
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        ui.set_max_width(320.0);
                        ui.label(RichText::new("Three things that aren't obvious").strong());
                        ui.label(
                            "⌘↑ (or ↑ at an empty prompt) browses past commands: ⏎ runs one again.",
                        );
                        ui.label("⌘I asks the AI about a command, an error or its output.");
                        ui.label("⇧⌘P finds any command, setting, theme or tab.");
                        if ui.button("Got It").clicked() {
                            done = true;
                        }
                    });
                });
            if done {
                self.welcome = false;
            }
        }
    }

    /// The tab bar shows with two or more tabs, or always when chosen (View
    /// → Show Tab Bar); Hide Tab Bar hides it even with several tabs.
    fn tab_bar_shown(&self) -> bool {
        !self.tab_bar_hidden && (self.w.tabs.len() > 1 || self.settings.show_tab_bar)
    }

    /// Opens Settings (at a pane), or brings it to the front.
    fn open_settings(&mut self, pane: Option<palette::Pane>) {
        let now = self.settings_open.unwrap_or_default();
        self.settings_open = Some(pane.unwrap_or(now));
        self.focus_settings = true;
    }

    /// A window's viewport (to bring it forward).
    fn viewport_of(w: &Win) -> egui::ViewportId {
        if w.root {
            egui::ViewportId::ROOT
        } else {
            egui::ViewportId::from_hash_of(("jamalterm-window", w.id))
        }
    }

    /// Settings changed: saved at once and applied (no Save button).
    fn settings_changed(&mut self) {
        self.settings = self.settings.clone().normalized();
        if crate::diag::enabled() != self.settings.keep_problem_log {
            crate::diag::set_enabled(self.settings.keep_problem_log);
        }
        self.theme = themes::by_id(&self.settings.theme);
        if let Err(e) = settings::save(&self.settings_path, &self.settings) {
            crate::diag::problem("settings", &format!("settings weren't saved: {e}"));
        }
    }

    /// Runs what was picked in the palette.
    fn run_action(&mut self, ctx: &egui::Context, a: palette::Action) {
        self.recent.retain(|r| *r != a);
        self.recent.insert(0, a.clone());
        self.recent.truncate(5);
        match a {
            palette::Action::Cmd(c) => self.run(ctx, c),
            palette::Action::Toggle(i) => {
                let t = &palette::TOGGLES[i];
                let v = (t.field)(&mut self.settings);
                *v = !*v;
                self.settings_changed();
            }
            palette::Action::Theme(id) => {
                self.settings.theme = id.to_string();
                self.settings_changed();
            }
            palette::Action::Tab(win, i) => {
                if win == self.w.id {
                    self.w.active = i.min(self.w.tabs.len().saturating_sub(1));
                } else if let Some(w) = self.others.iter_mut().find(|w| w.id == win) {
                    w.active = i.min(w.tabs.len().saturating_sub(1));
                    let vp = App::viewport_of(w);
                    ctx.send_viewport_cmd_to(vp, egui::ViewportCommand::Focus);
                }
            }
            palette::Action::Settings(p) => self.open_settings(Some(p)),
            palette::Action::Provider(id) => {
                self.settings.ai_provider = id.to_string();
                self.settings_changed();
            }
        }
    }

    /// The palette over the window: a box to type in and what matches.
    fn palette_ui(&mut self, ctx: &egui::Context) {
        let Some(mut p) = self.w.palette.take() else {
            return;
        };
        let mut items = palette::items(&self.settings);
        // Every tab, in every window.
        let many = !self.others.is_empty();
        let mut wins: Vec<&Win> = std::iter::once(&self.w).chain(&self.others).collect();
        wins.sort_by_key(|w| (!w.root, w.id));
        for (wn, w) in wins.iter().enumerate() {
            for (i, t) in w.tabs.iter().enumerate() {
                let name = tab_name(&t.folder(&self.home), &t.program(&self.shell_name));
                let title = if many {
                    format!("Tab {}: {name} (window {})", i + 1, wn + 1)
                } else {
                    format!("Tab {}: {name}", i + 1)
                };
                items.push(palette::Item {
                    title,
                    group: "Tab",
                    keys: String::new(),
                    action: palette::Action::Tab(w.id, i),
                });
            }
        }
        let shown = palette::filter(&items, &p.query, &self.recent);
        let (up, down, enter, esc) = ctx.input_mut(|i| {
            (
                i.consume_key(egui::Modifiers::NONE, Key::ArrowUp),
                i.consume_key(egui::Modifiers::NONE, Key::ArrowDown),
                i.key_pressed(Key::Enter),
                i.key_pressed(Key::Escape),
            )
        });
        if down && p.selected + 1 < shown.len() {
            p.selected += 1;
        }
        if up {
            p.selected = p.selected.saturating_sub(1);
        }
        let mut chosen = None;
        let mut close = esc;
        let width = (ctx.content_rect().width() - 40.0).clamp(240.0, 560.0);
        egui::Area::new(egui::Id::unique("palette"))
            .anchor(egui::Align2::CENTER_TOP, Vec2::new(0.0, 48.0))
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_width(width);
                    let edit = ui.add(
                        egui::TextEdit::singleline(&mut p.query)
                            .hint_text("Type a command, setting, theme or tab")
                            .desired_width(f32::INFINITY),
                    );
                    if p.focus {
                        edit.request_focus();
                        p.focus = false;
                    }
                    if edit.changed() {
                        p.selected = 0;
                    }
                    ui.separator();
                    if shown.is_empty() {
                        ui.label("Nothing matches");
                    }
                    egui::ScrollArea::vertical()
                        .max_height(320.0)
                        .show(ui, |ui| {
                            for (n, it) in shown.iter().enumerate().take(50) {
                                let sel = n == p.selected;
                                let r = ui
                                    .horizontal(|ui| {
                                        let l = ui.selectable_label(sel, it.title.as_str());
                                        ui.with_layout(
                                            egui::Layout::right_to_left(egui::Align::Center),
                                            |ui| {
                                                ui.weak(it.group);
                                                if !it.keys.is_empty() {
                                                    ui.monospace(it.keys.as_str());
                                                }
                                            },
                                        );
                                        l
                                    })
                                    .inner;
                                if sel {
                                    r.scroll_to_me(None);
                                }
                                if r.clicked() {
                                    chosen = Some(it.action.clone());
                                }
                            }
                        });
                });
            });
        if enter && let Some(it) = shown.get(p.selected) {
            chosen = Some(it.action.clone());
        }
        if chosen.is_some() {
            close = true;
        }
        if !close {
            self.w.palette = Some(p);
        }
        if let Some(a) = chosen {
            self.run_action(ctx, a);
        }
    }

    /// Quits: saves every window as it is, then closes the app.
    fn quit(&mut self, ctx: &egui::Context) {
        self.quitting = true;
        self.save_session(ctx);
        ctx.send_viewport_cmd_to(egui::ViewportId::ROOT, egui::ViewportCommand::Close);
    }

    /// ⇧⌘W, or the window's close button.
    fn close_window(&mut self, ctx: &egui::Context) {
        if self.w.root && self.others.is_empty() {
            // The last window: that's quitting (the session is kept).
            if self.may_close(ctx, Closing::Quit) {
                self.quit(ctx);
            }
        } else if self.may_close(ctx, Closing::Window { win: self.w.id }) {
            self.w.exiting = true;
        }
    }

    /// The programs (other than shells) running in what's closing, by name.
    /// Empty: nothing's running, close without asking.
    fn running_in(&self, c: Closing) -> Vec<String> {
        let wins: Vec<&Win> = std::iter::once(&self.w).chain(&self.others).collect();
        let mut panes: Vec<&Pane> = vec![];
        for w in wins {
            match c {
                Closing::Quit => panes.extend(w.tabs.iter().flat_map(|t| t.panes())),
                Closing::Window { win } if win == w.id => {
                    panes.extend(w.tabs.iter().flat_map(|t| t.panes()))
                }
                Closing::Tab { win, pane } if win == w.id => {
                    if let Some(t) = w
                        .tabs
                        .iter()
                        .find(|t| t.panes().iter().any(|p| p.id == pane))
                    {
                        panes.extend(t.panes());
                    }
                }
                Closing::Pane { win, pane } if win == w.id => panes.extend(
                    w.tabs
                        .iter()
                        .flat_map(|t| t.panes())
                        .filter(|p| p.id == pane),
                ),
                _ => {}
            }
        }
        let mut names: Vec<String> = panes
            .into_iter()
            .filter(|p| p.session.busy())
            .map(|p| {
                let s = p.session.screen.lock().unwrap();
                s.commands
                    .running()
                    .and_then(|b| program_of(&b.command))
                    .unwrap_or_else(|| "a program".into())
            })
            .collect();
        names.dedup();
        names
    }

    /// True when `c` may go ahead now: nothing's running in it, or it was
    /// just allowed. Otherwise asks (the close happens on "Terminate").
    fn may_close(&mut self, ctx: &egui::Context, c: Closing) -> bool {
        if self.allowed == Some(c) {
            self.allowed = None;
            return true;
        }
        if self.asking.is_some() {
            return false; // one question at a time
        }
        let running = self.running_in(c);
        if running.is_empty() {
            return true;
        }
        let detail = c.detail(&running);
        // The self-test can't click a native alert: it's asked in the window.
        let native = if self.smoke.is_some() {
            None
        } else {
            alert::ask_native(c, &detail, ctx)
        };
        self.asking = Some(alert::Pending {
            closing: c,
            detail,
            native,
        });
        false
    }

    /// The answer came: on "Terminate", do the close it was about.
    fn answered(&mut self, ctx: &egui::Context, c: Closing, terminate: bool) {
        self.asking = None;
        if !terminate {
            return;
        }
        self.allowed = Some(c);
        let win = match c {
            Closing::Quit => {
                self.quit(ctx);
                return;
            }
            Closing::Pane { win, .. } | Closing::Tab { win, .. } | Closing::Window { win } => win,
        };
        // Do it in that window.
        let found = if self.w.id != win {
            self.others.iter().position(|w| w.id == win)
        } else {
            Some(usize::MAX) // this window
        };
        let Some(i) = found else {
            return; // it's gone already
        };
        if i != usize::MAX {
            std::mem::swap(&mut self.w, &mut self.others[i]);
        }
        match c {
            Closing::Pane { pane, .. } | Closing::Tab { pane, .. } => {
                if let Some(t) = self
                    .w
                    .tabs
                    .iter()
                    .position(|t| t.panes().iter().any(|p| p.id == pane))
                {
                    if matches!(c, Closing::Pane { .. }) {
                        self.w.active = t;
                        self.w.tabs[t].focus = pane;
                        self.close_pane(ctx);
                    } else {
                        self.close_tab(ctx, t);
                    }
                }
            }
            Closing::Window { .. } => self.close_window(ctx),
            Closing::Quit => {}
        }
        if i != usize::MAX {
            std::mem::swap(&mut self.w, &mut self.others[i]);
        }
        self.allowed = None;
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
            Cmd::Settings => self.open_settings(None),
            Cmd::ShowTab(win, tab) => {
                if win == self.w.id {
                    self.w.active = tab.min(self.w.tabs.len().saturating_sub(1));
                    ctx.send_viewport_cmd_to(
                        App::viewport_of(&self.w),
                        egui::ViewportCommand::Focus,
                    );
                } else if let Some(w) = self.others.iter_mut().find(|w| w.id == win) {
                    w.active = tab.min(w.tabs.len().saturating_sub(1));
                    ctx.send_viewport_cmd_to(App::viewport_of(w), egui::ViewportCommand::Focus);
                }
            }
            Cmd::ShowAll => {
                for w in std::iter::once(&self.w).chain(&self.others) {
                    ctx.send_viewport_cmd_to(App::viewport_of(w), egui::ViewportCommand::Focus);
                }
            }
            Cmd::HideMenuBarIcon => {
                self.settings.menu_bar_icon = false;
                self.settings_changed();
            }
            Cmd::YourDay => self.your_day = true,
            Cmd::SecureInput => {
                self.settings.secure_input = !self.settings.secure_input;
                self.settings_changed();
            }
            Cmd::Diagnostics => self.diagnostics = Some(self.diagnostics_report()),
            Cmd::Quit => {
                if self.may_close(ctx, Closing::Quit) {
                    self.quit(ctx);
                }
            }
            Cmd::SearchWeb => {
                let text = self
                    .pane()
                    .and_then(|p| p.session.screen.lock().unwrap().term.selection_to_string());
                if let Some(t) = text.filter(|t| !t.trim().is_empty()) {
                    open_url(&format!(
                        "https://www.google.com/search?q={}",
                        url_encode(t.trim())
                    ));
                }
            }
            Cmd::Palette => {
                self.w.palette = Some(palette::Palette {
                    focus: true,
                    ..Default::default()
                });
            }
            Cmd::SelectAll => {
                if let Some(p) = self.pane() {
                    let mut s = p.session.screen.lock().unwrap();
                    let top = Point::new(Line(-(s.term.grid().history_size() as i32)), Column(0));
                    let bottom = Point::new(
                        Line(s.term.screen_lines() as i32 - 1),
                        Column(s.term.columns() - 1),
                    );
                    let mut sel = Selection::new(SelectionType::Simple, top, Side::Left);
                    sel.update(bottom, Side::Right);
                    s.term.selection = Some(sel);
                }
            }
            Cmd::FindSelection => {
                let text = self
                    .pane()
                    .and_then(|p| p.session.screen.lock().unwrap().term.selection_to_string());
                if let Some(t) = text.and_then(|t| t.lines().next().map(str::to_string))
                    && !t.trim().is_empty()
                {
                    let f = self.w.find.get_or_insert_with(Find::default);
                    f.query = t;
                    self.w.focus_find = true;
                }
            }
            Cmd::ToggleTabBar => {
                if self.tab_bar_shown() {
                    self.settings.show_tab_bar = false;
                    self.tab_bar_hidden = true;
                } else {
                    self.settings.show_tab_bar = true;
                    self.tab_bar_hidden = false;
                }
                let _ = settings::save(&self.settings_path, &self.settings);
            }
            Cmd::AiPanel => {
                self.w.ai_open = !self.w.ai_open;
                if self.w.ai_open {
                    self.w.ai.focus_input = true;
                }
            }
            Cmd::NewWindow => self.new_window(ctx),
            Cmd::CloseWindow => self.close_window(ctx),
            Cmd::MoveTabToNewWindow => self.move_tab_to_new_window(),
            Cmd::MergeAllWindows => self.merge_all_windows(),
            Cmd::SplitRight => self.split(ctx, Dir::Right),
            Cmd::SplitDown => self.split(ctx, Dir::Down),
            Cmd::ClosePane => self.close_pane(ctx),
            Cmd::CloseTab => self.close_tab(ctx, self.w.active),
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
                        if let Some(f) = self.w.find.as_mut() {
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
                self.w.find.get_or_insert_with(Find::default);
                self.w.focus_find = true;
            }
            Cmd::FindNext | Cmd::FindPrevious => {
                if let Some(f) = self.w.find.as_mut() {
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
            Cmd::PreviousTab => {
                self.w.active = (self.w.active + self.w.tabs.len() - 1) % self.w.tabs.len()
            }
            Cmd::NextTab => self.w.active = (self.w.active + 1) % self.w.tabs.len(),
            Cmd::Tab(n) => {
                if (n as usize) <= self.w.tabs.len() {
                    self.w.active = n as usize - 1;
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
        let Some(m) = self
            .w
            .find
            .as_ref()
            .and_then(|f| f.current_match().cloned())
        else {
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
        let Some(f) = self.w.find.as_mut() else {
            return;
        };
        ui.horizontal(|ui| {
            ui.label("Find");
            let edit = ui.add(
                egui::TextEdit::singleline(&mut f.query)
                    .desired_width(260.0)
                    .hint_text("text in the output"),
            );
            if self.w.focus_find {
                edit.request_focus();
                self.w.focus_find = false;
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
            .w
            .tabs
            .get(self.w.active)
            .is_some_and(|t| f.update(&t.pane().session.screen.lock().unwrap().term));
        if step != 0 {
            f.step(step);
        }
        if close {
            self.w.find = None;
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
        let mut rects = vec![];
        let mut drag_started = None;
        let bar = ui
            .horizontal(|ui| {
                for (i, tab) in self.w.tabs.iter().enumerate() {
                    let screen = tab.pane().session.screen.lock().unwrap();
                    let failed = screen.commands.running().is_none()
                        && screen
                            .commands
                            .last_finished()
                            .is_some_and(|b| b.exit != Some(0));
                    let running = screen.commands.running().is_some();
                    drop(screen);
                    let mut name =
                        tab_name(&tab.folder(&self.home), &tab.program(&self.shell_name));
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
                    let lock = if i == self.w.active && crate::secure::on() {
                        "🔒 "
                    } else {
                        ""
                    };
                    let mut text = RichText::new(format!("{}  {lock}{mark}{name}", i + 1));
                    if i == self.w.active {
                        text = text.strong();
                    }
                    let b = ui.add(
                        egui::Button::new(text)
                            .selected(i == self.w.active)
                            .min_size(Vec2::new(140.0, 24.0))
                            .sense(Sense::click_and_drag()),
                    );
                    rects.push(b.rect);
                    // Its project's color (tabs in the same git repository
                    // share one), or its ssh host's color from Settings.
                    let (cwd, running) = {
                        let s = tab.pane().session.screen.lock().unwrap();
                        (
                            s.cwd.clone(),
                            s.commands.running().map(|b| b.command.clone()),
                        )
                    };
                    let edge = running
                        .as_deref()
                        .and_then(|c| crate::fun::host_color(c, &self.settings.host_colors))
                        .or_else(|| {
                            self.settings
                                .project_colors
                                .then(|| crate::fun::repo_root(&cwd))
                                .flatten()
                                .map(|root| crate::fun::project_color(&root, &self.theme))
                        });
                    if let Some(c) = edge {
                        let r = b.rect;
                        ui.painter().rect_filled(
                            egui::Rect::from_min_size(
                                r.left_top() + Vec2::new(2.0, 4.0),
                                Vec2::new(3.0, r.height() - 8.0),
                            ),
                            1.5,
                            c,
                        );
                    }
                    if b.clicked() {
                        select = Some(i);
                    }
                    if b.middle_clicked() {
                        close = Some(i);
                    }
                    if b.drag_started() {
                        drag_started = Some(i);
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
            })
            .response
            .rect;
        if let Some(i) = drag_started {
            self.w.tab_drag = Some(i);
        }
        if self.w.root
            && let Some(s) = self.smoke.as_mut()
        {
            s.tab_rects = rects.clone();
        }
        if let Some(from) = self.w.tab_drag {
            self.drag_tab(ctx, ui, from, bar, &rects);
        }
        if let Some(i) = select {
            self.w.active = i;
        }
        if let Some(i) = close {
            self.close_tab(ctx, i);
        }
    }

    /// A tab being dragged: shows where it would go; on letting go, moves it
    /// (see tabdrag.rs). Esc puts it back.
    fn drag_tab(
        &mut self,
        ctx: &egui::Context,
        ui: &egui::Ui,
        from: usize,
        bar: egui::Rect,
        rects: &[egui::Rect],
    ) {
        let (pos, gone, released, esc) = ctx.input(|i| {
            (
                i.pointer.latest_pos(),
                i.pointer.hover_pos().is_none(),
                i.pointer.any_released(),
                i.key_pressed(Key::Escape),
            )
        });
        if esc || from >= self.w.tabs.len() {
            self.w.tab_drag = None;
            return;
        }
        let window = ctx.content_rect();
        if let Some(p) = pos {
            self.w.drag_pos = Some(p);
        }
        let Some(mut p) = self.w.drag_pos else {
            return;
        };
        if gone && window.contains(p) {
            // The pointer left the window: it's past the nearest edge.
            let to_edge = [
                p.x - window.left(),
                window.right() - p.x,
                p.y - window.top(),
                window.bottom() - p.y,
            ];
            let i = (0..4)
                .min_by(|a, b| to_edge[*a].total_cmp(&to_edge[*b]))
                .unwrap();
            let out = 40.0;
            match i {
                0 => p.x = window.left() - out,
                1 => p.x = window.right() + out,
                2 => p.y = window.top() - out,
                _ => p.y = window.bottom() + out,
            }
        }
        ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
        let others: Vec<(u64, restore::Frame)> = self
            .others
            .iter()
            .filter_map(|w| Some((w.id, w.frame?)))
            .collect();
        let target = tabdrag::drop_at(p, window, self.w.frame, bar, rects, &others);
        if let tabdrag::Drop::Reorder(to) = target {
            // A bar where it would land.
            let x = rects
                .get(to)
                .map(|r| r.left() - 3.0)
                .or_else(|| rects.last().map(|r| r.right() + 3.0))
                .unwrap_or(bar.left());
            ui.painter().line_segment(
                [egui::pos2(x, bar.top()), egui::pos2(x, bar.bottom())],
                egui::Stroke::new(3.0, self.theme.cursor),
            );
        }
        if !released {
            return;
        }
        self.w.tab_drag = None;
        self.w.drag_pos = None;
        match target {
            tabdrag::Drop::Reorder(to) => {
                self.w.active = tabdrag::reorder(&mut self.w.tabs, from, to);
            }
            tabdrag::Drop::ToWindow(id) => {
                let tab = self.w.tabs.remove(from);
                self.w.active = self.w.active.min(self.w.tabs.len().saturating_sub(1));
                if let Some(w) = self.others.iter_mut().find(|w| w.id == id) {
                    w.tabs.push(tab);
                    w.active = w.tabs.len() - 1;
                    self.front = id;
                }
                // An emptied window closes at the end of the frame.
            }
            tabdrag::Drop::NewWindow(at) if self.w.tabs.len() > 1 => {
                let tab = self.w.tabs.remove(from);
                self.w.active = self.w.active.min(self.w.tabs.len() - 1);
                let size = self.w.frame.map(|f| (f.w, f.h)).unwrap_or((900, 600));
                self.window_with(vec![tab]);
                if let Some(w) = self.others.last_mut() {
                    w.open_at = Some(restore::Frame {
                        x: at.x as i32 - 80,
                        y: at.y as i32 - 12,
                        w: size.0,
                        h: size.1,
                    });
                }
            }
            _ => {}
        }
    }
}

impl eframe::App for App {
    /// Quitting (⌘Q included) saves the session one last time.
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        let ctx = self.ctx.clone();
        self.save_session(&ctx);
        crate::secure::set(false);
        crate::diag::quit_cleanly();
    }

    /// The self-test's key presses arrive as if typed.
    fn raw_input_hook(&mut self, _ctx: &egui::Context, raw: &mut egui::RawInput) {
        if let Some(s) = self.smoke.as_mut() {
            raw.events.append(&mut s.keys);
            if let Some(mut batch) = s.script.pop_front() {
                raw.events.append(&mut batch);
            }
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        // The tab bar, palette and Settings are light or dark with the theme.
        let look = if self.theme.light {
            egui::Theme::Light
        } else {
            egui::Theme::Dark
        };
        if ctx.theme() != look {
            ctx.set_theme(look);
        }
        // JAMALTERM_TEST_CRASH=1: crash on purpose a few seconds in, to check
        // that the next start notices.
        if self.started.elapsed() > Duration::from_secs(3)
            && std::env::var_os("JAMALTERM_TEST_CRASH").is_some()
        {
            panic!("a test crash (JAMALTERM_TEST_CRASH)");
        }
        // Menu bar choices (and their key equivalents) on a Mac go to the
        // window in front.
        #[cfg(target_os = "macos")]
        if let Some(m) = &self.menu {
            self.pending.extend(m.chosen());
        }
        let ids: Vec<u64> = std::iter::once(self.w.id)
            .chain(self.others.iter().map(|w| w.id))
            .collect();
        if !ids.contains(&self.front) && !(self.front == SETTINGS && self.settings_open.is_some()) {
            self.front = self.w.id;
        }

        // The first window (eframe's root viewport).
        let was_active = std::mem::take(&mut self.app_active);
        self.window_ui(ui);
        if self
            .smoke
            .as_mut()
            .is_some_and(|s| std::mem::take(&mut s.close_settings))
        {
            self.close_settings();
        }
        if let Some(run) = self.smoke.as_mut().and_then(|s| s.ai_decide.take()) {
            self.ai_act(&ctx, aipanel::Act::Decide(run));
        }
        if self.w.root
            && let Some(smoke) = self.smoke.as_mut()
        {
            smoke.find_status = self.w.find.as_ref().map(|f| f.status());
            smoke.windows = 1 + self.others.len();
            smoke.palette_open = self.w.palette.is_some();
            smoke.settings_open = self.settings_open.is_some();
            smoke.theme = self.settings.theme.clone();
            smoke.asking = self.asking.is_some();
            smoke.crash_noticed = self.last_crash.is_some();
            smoke.stats_programs = self.stats.runs.iter().map(|r| r.program.clone()).collect();
            smoke.ai_open = self.w.ai_open;
            smoke.ai_card = self.w.ai.items.iter().rev().find_map(|i| match i {
                aipanel::Item::Tool(c) => Some(format!("{:?}", c.state)),
                _ => None,
            });
            smoke.ai_answer = self.w.ai.items.iter().rev().find_map(|i| match i {
                aipanel::Item::Answer(a) => Some(a.clone()),
                aipanel::Item::Error(e) => Some(format!("error: {e}")),
                _ => None,
            });
            smoke.ai_busy = self.w.ai.busy();
            smoke.step(&ctx, &self.w.tabs, self.w.active);
        }

        if self.smoke.as_ref().is_some_and(|s| s.quit) && !self.quitting {
            self.quit(&ctx);
        }

        // The others, each in its own native window: swapped in, drawn,
        // swapped back.
        let mut i = 0;
        while i < self.others.len() {
            std::mem::swap(&mut self.w, &mut self.others[i]);
            let id = egui::ViewportId::from_hash_of(("jamalterm-window", self.w.id));
            let mut builder = egui::ViewportBuilder::default()
                .with_title("jamalterm")
                .with_inner_size([900.0, 600.0])
                .with_min_inner_size([480.0, 300.0]);
            if let Some(f) = self.w.open_at {
                builder = builder
                    .with_position([f.x as f32, f.y as f32])
                    .with_inner_size([f.w.max(480) as f32, f.h.max(300) as f32]);
            }
            ctx.show_viewport_immediate(id, builder, |ui, _| self.window_ui(ui));
            std::mem::swap(&mut self.w, &mut self.others[i]);
            let w = &self.others[i];
            if w.tabs.is_empty() || w.exiting {
                // Closed: forgotten, and its shells end. Its native window
                // closes because it isn't drawn any more.
                self.others.remove(i);
            } else {
                i += 1;
            }
        }

        self.settings_window(&ctx);
        self.diagnostics_window(&ctx);
        self.your_day_window(&ctx);
        self.finished_commands();
        #[cfg(target_os = "macos")]
        self.update_switcher();
        self.secure_input(was_active);

        // A "terminate running processes?" question: its answer.
        if let Some(p) = &self.asking {
            let c = p.closing;
            let smoke_answer = self.smoke.as_mut().and_then(|s| s.answer.take());
            let answer = match &p.native {
                Some(rx) => match rx.try_recv() {
                    Ok(a) => Some(a),
                    Err(mpsc::TryRecvError::Disconnected) => Some(false),
                    Err(mpsc::TryRecvError::Empty) => None,
                },
                None => smoke_answer.or_else(|| alert::ask_in_window(&ctx, p)),
            };
            if let Some(a) = answer {
                self.answered(&ctx, c, a);
            }
        }

        // The first window closed (or lost its last tab) while others are
        // open: it takes over another window's tabs, so the app goes on.
        if (self.w.exiting || self.w.tabs.is_empty()) && !self.others.is_empty() {
            let mut next = self.others.remove(0);
            next.root = true;
            next.open_at = None;
            self.front = next.id;
            self.w = next; // the old first window's tabs end here
        }

        // Saved every 15 s when something changed (and on quit), so a crash
        // loses little.
        let smoke_wants = self
            .smoke
            .as_ref()
            .is_some_and(|s| s.save_session && self.last_saved.is_empty());
        if smoke_wants || self.saved_at.elapsed() > Duration::from_secs(15) {
            self.save_session(&ctx);
        }
        if self.settings.cursor_blink {
            ctx.request_repaint_after(Duration::from_millis(530));
        }
    }
}

impl App {
    /// Draws the window in `self.w` and handles its keys and mouse.
    fn window_ui(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        if self.w.tabs.is_empty() {
            return;
        }
        let font_size = self.settings.font_size as f32;
        let cell = view::cell_size(&ctx, font_size, self.settings.line_height);

        // A new prompt: nothing typed on it yet.
        for t in &mut self.w.tabs {
            for pane in t.tree_mut().leaves_mut() {
                let p = pane.session.screen.lock().unwrap().commands.prompts;
                if p != pane.prompts {
                    pane.prompts = p;
                    pane.typed = false;
                }
            }
        }

        // Where it is, and whether it's in front (menu commands go there).
        let (focused, close, frame) = ctx.input(|i| {
            let v = i.viewport();
            let frame = v.outer_rect.zip(v.inner_rect).map(|(o, r)| restore::Frame {
                x: o.min.x.round() as i32,
                y: o.min.y.round() as i32,
                w: r.width().round() as i32,
                h: r.height().round() as i32,
            });
            (v.focused == Some(true), v.close_requested(), frame)
        });
        if frame.is_some() {
            self.w.frame = frame;
        }
        if focused {
            self.front = self.w.id;
            self.app_active = true;
        }
        if self.front == self.w.id {
            for cmd in std::mem::take(&mut self.pending) {
                self.run(&ctx, cmd);
            }
        }
        if self.w.root
            && let Some(cmds) = self.smoke.as_mut().map(|s| std::mem::take(&mut s.cmds))
        {
            for cmd in cmds {
                self.run(&ctx, cmd);
            }
        }
        if close {
            let last = self.w.root && self.others.is_empty();
            let c = if last {
                Closing::Quit
            } else {
                Closing::Window { win: self.w.id }
            };
            if !self.quitting && !self.may_close(&ctx, c) {
                // Asked first; it closes on "Terminate".
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            } else if self.quitting || last {
                // The last window: quitting. The session is kept.
                self.save_session(&ctx);
            } else {
                // One of several: it's forgotten (the first window takes
                // over another's tabs rather than close).
                if self.w.root {
                    ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                }
                self.w.exiting = true;
            }
        }
        if self.w.tabs.is_empty() || self.w.exiting {
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
                    // The find box, palette or a question has the keys. (egui
                    // drops a field's focus on Esc before this runs, so Esc
                    // that closes one of them must not reach the shell.)
                    if typing_in_field
                        || self.asking.is_some()
                        || (key == Key::Escape
                            && (self.w.find.is_some() || self.w.palette.is_some()))
                    {
                        continue;
                    }
                    // Browse mode takes the keys while it's on.
                    if self
                        .w
                        .tabs
                        .get(self.w.active)
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
        if self.w.tabs.is_empty() {
            return;
        }

        self.crash_notice(ui);
        if self.tab_bar_shown() {
            egui::Panel::top("tabs").show(ui, |ui| self.tab_bar(&ctx, ui));
        }
        if self.w.tabs.is_empty() {
            return;
        }

        if self.w.find.is_some() {
            egui::Panel::top("find").show(ui, |ui| self.find_bar(ui));
        }
        self.palette_ui(&ctx);
        if self.w.tabs.is_empty() {
            return;
        }
        self.cards(&ctx);
        self.ai_panel(&ctx, ui);

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
                self.w.grid = view::grid_size(area, cell);
                let a = self.w.active;
                // Where the panes and dividers go.
                let (mut placed, mut divs) = (vec![], vec![]);
                self.w.tabs[a].tree().layout(area, &mut placed, &mut divs);
                let placed: Vec<(u64, egui::Rect)> =
                    placed.into_iter().map(|(p, r)| (p.id, r)).collect();
                let multi = placed.len() > 1;
                let focus = self.w.tabs[a].pane().id;
                let window_focused = ctx.input(|i| i.focused);
                let smooth =
                    self.settings.cursor_effect == "smooth" && !crate::macos::reduce_motion();
                let mut focus_to = None;
                let mut click_browse = None;
                let mut context: Option<bool> = None; // right-clicked (with a selection?)
                #[cfg_attr(target_os = "macos", allow(unused_mut))]
                let mut context_cmd: Option<Cmd> = None;
                for &(id, rect) in &placed {
                    let response = ui.interact(
                        rect,
                        egui::Id::unique(("pane", id)),
                        Sense::click_and_drag(),
                    );
                    let focused = id == focus;
                    let Some(pane) = self.w.tabs[a].tree_mut().find_mut(&|p: &Pane| p.id == id)
                    else {
                        continue;
                    };
                    let grid = view::grid_size(rect, cell);
                    if grid != pane.grid {
                        pane.grid = grid;
                        pane.session.resize(grid.0, grid.1);
                    }
                    let mut screen = pane.session.screen.lock().unwrap();
                    let selection = screen.term.selection.is_some();
                    if response.secondary_clicked() {
                        // Right-click: the menu (and the pane gets the focus).
                        focus_to = Some(id);
                        context = Some(selection);
                    }
                    #[cfg(not(target_os = "macos"))]
                    response.context_menu(|ui| {
                        for item in commands::context_menu(selection) {
                            match item {
                                Some((cmd, title, enabled)) => {
                                    if ui.add_enabled(enabled, egui::Button::new(title)).clicked() {
                                        context_cmd = Some(cmd);
                                        ui.close();
                                    }
                                }
                                None => {
                                    ui.separator();
                                }
                            }
                        }
                    });
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
                    // Smooth cursor: it glides toward where it is (not with
                    // Reduce Motion).
                    let cursor_at = if smooth {
                        match (pane.cursor_drawn, pane.cursor_real) {
                            (Some(d), Some(r))
                                if d.distance(r) > 0.5 && d.distance(r) < cell.x * 40.0 =>
                            {
                                ctx.request_repaint();
                                Some(d + (r - d) * 0.35)
                            }
                            (_, r) => r,
                        }
                    } else {
                        None
                    };
                    let look = Look {
                        theme: &self.theme,
                        font_size,
                        cell,
                        cursor_style: &self.settings.cursor_style,
                        focused: window_focused && focused,
                        blink_on,
                        browse: browse_look,
                        cursor_at,
                        glow: self.settings.cursor_effect == "glow",
                    };
                    let painter = ui.painter_at(rect);
                    let painted = view::paint(&painter, rect, &screen, &look);
                    let origin = painted.origin;
                    pane.cursor_drawn = cursor_at;
                    pane.cursor_real = painted.cursor.map(|r| r.min);

                    if let Some(pos) = response.interact_pointer_pos() {
                        let (point, side) = view::point_at(pos, origin, cell, &screen);
                        if !focused && (response.clicked() || response.drag_started()) {
                            // A click in another pane gives it the focus.
                            focus_to = Some(id);
                        }
                        if response.drag_started() {
                            screen.term.selection =
                                Some(Selection::new(SelectionType::Simple, point, side));
                            self.w.selecting = true;
                        } else if response.dragged() && self.w.selecting {
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
                        self.w.selecting = false;
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
                        self.w.tabs[a].tree_mut().equalize(d);
                    } else if r.drag_started() {
                        self.w.dragging = Some(d.clone());
                    }
                    if r.dragged()
                        && let (Some(pos), Some(dd)) = (r.interact_pointer_pos(), &self.w.dragging)
                    {
                        let dd = dd.clone();
                        self.w.tabs[a].tree_mut().drag(&dd, pos);
                    }
                    if r.drag_stopped() {
                        self.w.dragging = None;
                    }
                }

                if let Some(id) = focus_to {
                    self.w.tabs[a].focus = id;
                }
                #[cfg(target_os = "macos")]
                if let Some(sel) = context {
                    crate::menu::show_context_menu(sel);
                }
                let _ = context;
                if let Some(cmd) = context_cmd {
                    self.run(&ctx, cmd);
                }
                if let Some((id, row, col)) = click_browse
                    && let Some(p) = self.w.tabs[a].tree_mut().find_mut(&|p: &Pane| p.id == id)
                {
                    p.browse = Some(Browse::new(row, col));
                }
                self.w.pane_rects = placed;
            });

        // The window title: "Home — zsh".
        let tab = &self.w.tabs[self.w.active];
        let mut title = tab_name(&tab.folder(&self.home), &tab.program(&self.shell_name));
        if crate::secure::on() && self.front == self.w.id {
            title = format!("🔒 {title}"); // Secure Keyboard Entry is on
        }
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(title));
        #[cfg(target_os = "macos")]
        if self.front == self.w.id
            && let Some(m) = &self.menu
        {
            let split = self.w.tabs[self.w.active].tree().count() > 1;
            m.set_title(
                Cmd::ClosePane,
                if split { "Close Pane" } else { "Close Tab" },
            );
            let shown = self.w.tabs.len() > 1 || self.settings.show_tab_bar;
            m.set_title(
                Cmd::AiPanel,
                if self.w.ai_open {
                    "Hide AI Panel"
                } else {
                    "Show AI Panel"
                },
            );
            m.set_title(
                Cmd::ToggleTabBar,
                if shown {
                    "Hide Tab Bar"
                } else {
                    "Show Tab Bar"
                },
            );
        }
    }
}

/// This computer's offset from UTC, in seconds (for "today").
fn local_utc_offset() -> i64 {
    let now = unsafe { libc::time(std::ptr::null_mut()) };
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    unsafe { libc::localtime_r(&now, &mut tm) };
    tm.tm_gmtoff as i64
}

/// "macOS 15.2" (or the system's name elsewhere).
fn os_version() -> String {
    if cfg!(target_os = "macos")
        && let Ok(o) = std::process::Command::new("/usr/bin/sw_vers")
            .arg("-productVersion")
            .output()
    {
        return format!("macOS {}", String::from_utf8_lossy(&o.stdout).trim());
    }
    std::env::consts::OS.to_string()
}

/// Opens a web page in the browser (a click the user made).
fn open_url(url: &str) {
    let opener = if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    let _ = std::process::Command::new(opener).arg(url).spawn();
}

/// Text for a URL's query.
fn url_encode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            b' ' => "+".into(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// `App::front` while the Settings window is in front.
const SETTINGS: u64 = 0;

/// A tab's tree as it's saved.
fn save_node(n: &Node<Pane>, focus: u64) -> restore::SavedNode {
    match n {
        Node::Leaf(p) => {
            let s = p.session.screen.lock().unwrap();
            restore::SavedNode::Leaf {
                tab: restore::SavedTab {
                    cwd: s.cwd.clone(),
                    output: restore::serialize(&s.term),
                    focused: p.id == focus,
                    ..Default::default()
                },
            }
        }
        Node::Split { dir, ratio, a, b } => restore::SavedNode::Split {
            dir: restore::dir_name(*dir).into(),
            ratio: *ratio,
            a: Box::new(save_node(a, focus)),
            b: Box::new(save_node(b, focus)),
        },
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
    fn search_text_is_encoded() {
        assert_eq!(url_encode("rm -rf ~/x&y"), "rm+-rf+~%2Fx%26y");
        assert_eq!(url_encode("é"), "%C3%A9");
    }

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
