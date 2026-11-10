//! The window: tabs, the tab bar, keyboard and mouse, and the terminal view.

use crate::input::{is_app_shortcut, key_bytes, paste_bytes};
use crate::session::Session;
use crate::settings::{self, Settings};
use crate::shell::ShellSpec;
use crate::themes::{self, Theme};
use crate::view::{self, Look};
use alacritty_terminal::grid::Scroll;
use alacritty_terminal::selection::{Selection, SelectionType};
use alacritty_terminal::term::TermMode;
use egui::{Color32, Key, RichText, Sense, Vec2};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

pub struct Tab {
    pub session: Session,
}

impl Tab {
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
        };
        app.new_tab(&cc.egui_ctx, None);
        app
    }

    fn new_tab(&mut self, ctx: &egui::Context, dir: Option<PathBuf>) {
        // New tabs open in the current tab's folder, like Terminal.
        let dir = dir.or_else(|| {
            let cwd = self
                .tabs
                .get(self.active)?
                .session
                .screen
                .lock()
                .unwrap()
                .cwd
                .clone();
            (!cwd.is_empty()).then(|| PathBuf::from(cwd))
        });
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
            self.grid.0,
            self.grid.1,
            self.settings.scrollback,
            repaint,
        ) {
            Ok(session) => {
                self.tabs.push(Tab { session });
                self.active = self.tabs.len() - 1;
            }
            Err(e) => eprintln!("jamalterm: the shell didn't start: {e}"),
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
        match key {
            Key::T => self.new_tab(ctx, None),
            Key::W => self.close_tab(ctx, self.active),
            Key::K => {
                // Clear scrollback (and the screen), like Terminal's ⌘K.
                if let Some(t) = self.tabs.get(self.active) {
                    t.session.write(b"\x0c");
                    let mut s = t.session.screen.lock().unwrap();
                    s.term.grid_mut().clear_history();
                    s.commands.blocks.clear();
                }
            }
            Key::Equals | Key::Plus => self.set_font_size(self.settings.font_size + 1),
            Key::Minus => self.set_font_size(self.settings.font_size.saturating_sub(1)),
            Key::Num0 => self.set_font_size(Settings::default().font_size),
            Key::OpenBracket if m.shift => {
                self.active = (self.active + self.tabs.len() - 1) % self.tabs.len()
            }
            Key::CloseBracket if m.shift => self.active = (self.active + 1) % self.tabs.len(),
            Key::Num1
            | Key::Num2
            | Key::Num3
            | Key::Num4
            | Key::Num5
            | Key::Num6
            | Key::Num7
            | Key::Num8
            | Key::Num9 => {
                let n = key.name().parse::<usize>().unwrap_or(1) - 1;
                if n < self.tabs.len() {
                    self.active = n;
                }
            }
            _ => return false,
        }
        true
    }

    fn tab_bar(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        let mut select = None;
        let mut close = None;
        ui.horizontal(|ui| {
            for (i, tab) in self.tabs.iter().enumerate() {
                let screen = tab.session.screen.lock().unwrap();
                let failed = screen.commands.running().is_none()
                    && screen
                        .commands
                        .last_finished()
                        .is_some_and(|b| b.exit != Some(0));
                let running = screen.commands.running().is_some();
                drop(screen);
                let name = tab_name(&tab.folder(&self.home), &tab.program(&self.shell_name));
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
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        if self.tabs.is_empty() {
            return;
        }
        let font_size = self.settings.font_size as f32;
        let cell = view::cell_size(&ctx, font_size, self.settings.line_height);

        // Keyboard: app shortcuts first, then the shell.
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
                        continue;
                    }
                    let Some(tab) = self.tabs.get(self.active) else {
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
                    if let Some(bytes) =
                        key_bytes(key, &modifiers, app_cursor, self.settings.option_is_meta)
                    {
                        let mut s = tab.session.screen.lock().unwrap();
                        s.term.scroll_display(Scroll::Bottom); // typing jumps back to the prompt
                        s.term.selection = None;
                        drop(s);
                        tab.session.write(&bytes);
                    }
                }
                egui::Event::Text(t) => {
                    if let Some(tab) = self.tabs.get(self.active) {
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
                    if let Some(tab) = self.tabs.get(self.active) {
                        let bracketed = tab
                            .session
                            .screen
                            .lock()
                            .unwrap()
                            .term
                            .mode()
                            .contains(TermMode::BRACKETED_PASTE);
                        tab.session.write(&paste_bytes(&t, bracketed));
                    }
                }
                egui::Event::Copy => {
                    if let Some(tab) = self.tabs.get(self.active)
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

        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ui, |ui| {
                let rect = ui.max_rect();
                let response = ui.allocate_rect(rect, Sense::click_and_drag());
                let grid = view::grid_size(rect, cell);
                if grid != self.grid {
                    self.grid = grid;
                    for t in &self.tabs {
                        t.session.resize(grid.0, grid.1);
                    }
                }
                let tab = &self.tabs[self.active];
                let mut screen = tab.session.screen.lock().unwrap();

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
                                tab.session.write(key);
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
                    focused: ctx.input(|i| i.focused),
                    blink_on,
                };
                let origin = view::paint(ui.painter(), rect, &screen, &look);

                // Selecting text with the mouse.
                if let Some(pos) = response.interact_pointer_pos() {
                    let (point, side) = view::point_at(pos, origin, cell, &screen);
                    if response.drag_started() {
                        screen.term.selection =
                            Some(Selection::new(SelectionType::Simple, point, side));
                        self.selecting = true;
                    } else if response.dragged() && self.selecting {
                        if let Some(sel) = screen.term.selection.as_mut() {
                            sel.update(point, side);
                        }
                    } else if response.clicked() {
                        screen.term.selection = None;
                    }
                }
                if response.drag_stopped() {
                    self.selecting = false;
                }
                if let Some(code) = screen.exited {
                    ui.painter().text(
                        rect.center_bottom() - Vec2::new(0.0, 24.0),
                        egui::Align2::CENTER_CENTER,
                        format!("[the shell exited with code {code} — ⌘W to close]"),
                        view::mono(font_size * 0.9),
                        Color32::GRAY,
                    );
                }
            });

        // The window title: "Home — zsh".
        let tab = &self.tabs[self.active];
        let title = tab_name(&tab.folder(&self.home), &tab.program(&self.shell_name));
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(title));
        if self.settings.cursor_blink {
            ctx.request_repaint_after(Duration::from_millis(530));
        }
        if let Some(smoke) = self.smoke.as_mut() {
            smoke.step(&ctx, &self.tabs, self.active);
        }
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
