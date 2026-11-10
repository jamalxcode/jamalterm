//! The Mac menu bar (jamalterm, Shell, Edit, View, Window), built from the
//! command table. A menu item's shortcut is a real key equivalent, so the
//! menu title flashes when it's pressed, as in any Mac app. Clicks and key
//! equivalents arrive as command ids on a channel the app reads each frame.

use crate::commands::{self, Cmd, Menu};
use muda::accelerator::{Accelerator, Code, Modifiers};
use muda::{AboutMetadata, IsMenuItem, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};
use std::cell::RefCell;
use std::sync::mpsc;

pub struct MacMenu {
    _bar: muda::Menu,
    rx: mpsc::Receiver<Cmd>,
    /// Every command's item, to retitle ("Close Pane", "Hide Tab Bar"…).
    by_cmd: Vec<(Cmd, MenuItem, RefCell<String>)>,
}

/// A separator goes before these (groups within a menu).
const BREAK_BEFORE: &[Cmd] = &[
    Cmd::SplitRight,
    Cmd::ClosePane,
    Cmd::Find,
    Cmd::Browse,
    Cmd::Bigger,
    Cmd::PreviousPane,
    Cmd::MoveTabToNewWindow,
];

fn code(key: egui::Key) -> Option<Code> {
    use egui::Key as K;
    Some(match key {
        K::T => Code::KeyT,
        K::A => Code::KeyA,
        K::E => Code::KeyE,
        K::P => Code::KeyP,
        K::Comma => Code::Comma,
        K::N => Code::KeyN,
        K::W => Code::KeyW,
        K::C => Code::KeyC,
        K::V => Code::KeyV,
        K::F => Code::KeyF,
        K::G => Code::KeyG,
        K::K => Code::KeyK,
        K::D => Code::KeyD,
        K::ArrowUp => Code::ArrowUp,
        K::ArrowDown => Code::ArrowDown,
        K::ArrowLeft => Code::ArrowLeft,
        K::ArrowRight => Code::ArrowRight,
        K::Equals => Code::Equal,
        K::Minus => Code::Minus,
        K::Num0 => Code::Digit0,
        K::OpenBracket => Code::BracketLeft,
        K::CloseBracket => Code::BracketRight,
        _ => return None,
    })
}

fn item(e: &commands::Entry) -> MenuItem {
    let mut mods = Modifiers::META;
    if e.shift {
        mods |= Modifiers::SHIFT;
    }
    if e.alt {
        mods |= Modifiers::ALT;
    }
    let accel = e.key.and_then(code).map(|c| Accelerator::new(mods, c));
    MenuItem::with_id(commands::id(e.cmd), e.title, true, accel)
}

impl MacMenu {
    /// Builds the menu bar and makes it the app's. Call once, on the main
    /// thread, after the app has started.
    pub fn install(ctx: &egui::Context) -> Option<MacMenu> {
        let (tx, rx) = mpsc::channel();
        let ctx = ctx.clone();
        MenuEvent::set_event_handler(Some(move |e: MenuEvent| {
            if let Some(cmd) = commands::by_id(e.id.as_ref()) {
                let _ = tx.send(cmd);
                ctx.request_repaint();
            }
        }));

        let sep = PredefinedMenuItem::separator;
        let all: Vec<(Cmd, Menu, MenuItem)> = commands::TABLE
            .iter()
            .filter_map(|e| Some((e.cmd, e.menu?, item(e))))
            .collect();
        // A menu's items with separators between the groups.
        let seps: Vec<PredefinedMenuItem> = (0..BREAK_BEFORE.len()).map(|_| sep()).collect();
        let menu = |m: Menu| -> Vec<&dyn IsMenuItem> {
            let mut v: Vec<&dyn IsMenuItem> = vec![];
            for (cmd, menu, it) in &all {
                if *menu != m {
                    continue;
                }
                if let Some(i) = BREAK_BEFORE.iter().position(|c| c == cmd)
                    && !v.is_empty()
                {
                    v.push(&seps[i]);
                }
                v.push(it);
            }
            v
        };

        let about = AboutMetadata {
            name: Some("jamalterm".into()),
            version: Some(env!("CARGO_PKG_VERSION").into()),
            ..Default::default()
        };
        let (about, s0, s1, s2, s3) = (
            PredefinedMenuItem::about(Some("About jamalterm"), Some(about)),
            sep(),
            sep(),
            sep(),
            sep(),
        );
        let (services, hide, others, show, quit) = (
            PredefinedMenuItem::services(None),
            PredefinedMenuItem::hide(Some("Hide jamalterm")),
            PredefinedMenuItem::hide_others(None),
            PredefinedMenuItem::show_all(None),
            PredefinedMenuItem::quit(Some("Quit jamalterm")),
        );
        let mut app_items: Vec<&dyn IsMenuItem> = vec![&about, &s0];
        app_items.extend(menu(Menu::App)); // Settings…
        app_items.extend([
            &s1 as &dyn IsMenuItem,
            &services,
            &s2,
            &hide,
            &others,
            &show,
            &s3,
            &quit,
        ]);
        let app = Submenu::with_items("jamalterm", true, &app_items).ok()?;
        let shell = Submenu::with_items("Shell", true, &menu(Menu::Shell)).ok()?;
        let edit = Submenu::with_items("Edit", true, &menu(Menu::Edit)).ok()?;

        let full = PredefinedMenuItem::fullscreen(Some("Enter Full Screen"));
        let s4 = sep();
        let mut view = menu(Menu::View);
        view.extend([&s4 as &dyn IsMenuItem, &full]);
        let view = Submenu::with_items("View", true, &view).ok()?;

        let (min, zoom, s5, s6, front) = (
            PredefinedMenuItem::minimize(None),
            PredefinedMenuItem::maximize(Some("Zoom")),
            sep(),
            sep(),
            PredefinedMenuItem::bring_all_to_front(None),
        );
        let mut win: Vec<&dyn IsMenuItem> = vec![&min, &zoom, &s5];
        win.extend(menu(Menu::Window));
        win.extend([&s6 as &dyn IsMenuItem, &front]);
        let window = Submenu::with_items("Window", true, &win).ok()?;

        let bar = muda::Menu::with_items(&[&app, &shell, &edit, &view, &window]).ok()?;
        bar.init_for_nsapp();
        window.set_as_windows_menu_for_nsapp();
        let by_cmd = all
            .iter()
            .map(|(c, _, it)| (*c, it.clone(), RefCell::new(it.text())))
            .collect();
        Some(MacMenu {
            _bar: bar,
            rx,
            by_cmd,
        })
    }

    /// Menu items say what they'll do ("Close Pane" in a split tab, "Hide
    /// Tab Bar" while it's shown): retitles an item when that changes.
    pub fn set_title(&self, cmd: Cmd, title: &str) {
        if let Some((_, it, now)) = self.by_cmd.iter().find(|(c, ..)| *c == cmd)
            && *now.borrow() != title
        {
            it.set_text(title);
            *now.borrow_mut() = title.to_string();
        }
    }

    /// Commands chosen since the last frame.
    pub fn chosen(&self) -> Vec<Cmd> {
        self.rx.try_iter().collect()
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn every_menu_item_has_a_key_equivalent() {
        for e in crate::commands::TABLE.iter().filter(|e| e.menu.is_some()) {
            if let Some(k) = e.key {
                assert!(super::code(k).is_some(), "{:?}", e.cmd);
            }
        }
    }
}
