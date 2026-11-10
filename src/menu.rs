//! The Mac menu bar (jamalterm, Shell, Edit, View, Window), built from the
//! command table. A menu item's shortcut is a real key equivalent, so the
//! menu title flashes when it's pressed, as in any Mac app. Clicks and key
//! equivalents arrive as command ids on a channel the app reads each frame.

use crate::commands::{self, Cmd, Menu};
use muda::accelerator::{Accelerator, Code, Modifiers};
use muda::{AboutMetadata, IsMenuItem, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};
use std::sync::mpsc;

pub struct MacMenu {
    _bar: muda::Menu,
    rx: mpsc::Receiver<Cmd>,
    /// ⌘W, titled "Close Pane" while the tab is split.
    close: Option<MenuItem>,
    split: std::cell::Cell<bool>,
}

fn code(key: egui::Key) -> Option<Code> {
    use egui::Key as K;
    Some(match key {
        K::T => Code::KeyT,
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

fn items(menu: Menu) -> Vec<MenuItem> {
    commands::TABLE
        .iter()
        .filter(|e| e.menu == Some(menu))
        .map(|e| {
            let mut mods = Modifiers::META;
            if e.shift {
                mods |= Modifiers::SHIFT;
            }
            if e.alt {
                mods |= Modifiers::ALT;
            }
            let accel = e.key.and_then(code).map(|c| Accelerator::new(mods, c));
            MenuItem::with_id(commands::id(e.cmd), e.title, true, accel)
        })
        .collect()
}

fn refs(v: &[MenuItem]) -> Vec<&dyn IsMenuItem> {
    v.iter().map(|i| i as &dyn IsMenuItem).collect()
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
        let about = AboutMetadata {
            name: Some("jamalterm".into()),
            version: Some(env!("CARGO_PKG_VERSION").into()),
            ..Default::default()
        };
        let app = Submenu::with_items(
            "jamalterm",
            true,
            &[
                &PredefinedMenuItem::about(Some("About jamalterm"), Some(about)),
                &sep(),
                &PredefinedMenuItem::services(None),
                &sep(),
                &PredefinedMenuItem::hide(Some("Hide jamalterm")),
                &PredefinedMenuItem::hide_others(None),
                &PredefinedMenuItem::show_all(None),
                &sep(),
                &PredefinedMenuItem::quit(Some("Quit jamalterm")),
            ],
        )
        .ok()?;

        let shell = items(Menu::Shell);
        let close = shell
            .iter()
            .find(|i| i.id().as_ref() == commands::id(Cmd::ClosePane))
            .cloned();
        let shell = Submenu::with_items("Shell", true, &refs(&shell)).ok()?;

        // Edit: Copy, Paste | Find… Find Next, Find Previous | Browse, Clear.
        let edit_items = items(Menu::Edit);
        let mut edit: Vec<&dyn IsMenuItem> = vec![];
        let (s1, s2) = (sep(), sep());
        for (i, it) in edit_items.iter().enumerate() {
            if i == 2 {
                edit.push(&s1);
            }
            if i == 5 {
                edit.push(&s2);
            }
            edit.push(it);
        }
        let edit = Submenu::with_items("Edit", true, &edit).ok()?;

        let view = items(Menu::View);
        let full = PredefinedMenuItem::fullscreen(Some("Enter Full Screen"));
        let mut view_refs = refs(&view);
        let s3 = sep();
        view_refs.extend([&s3 as &dyn IsMenuItem, &full]);
        let view = Submenu::with_items("View", true, &view_refs).ok()?;

        let win = items(Menu::Window);
        let (min, zoom, s4, s5, front) = (
            PredefinedMenuItem::minimize(None),
            PredefinedMenuItem::maximize(Some("Zoom")),
            sep(),
            sep(),
            PredefinedMenuItem::bring_all_to_front(None),
        );
        let mut win_refs: Vec<&dyn IsMenuItem> = vec![&min, &zoom, &s4];
        win_refs.extend(refs(&win));
        win_refs.extend([&s5 as &dyn IsMenuItem, &front]);
        let window = Submenu::with_items("Window", true, &win_refs).ok()?;

        let bar = muda::Menu::with_items(&[&app, &shell, &edit, &view, &window]).ok()?;
        bar.init_for_nsapp();
        window.set_as_windows_menu_for_nsapp();
        Some(MacMenu {
            _bar: bar,
            rx,
            close,
            split: std::cell::Cell::new(false),
        })
    }

    /// Menu items say what they'll do: ⌘W is "Close Pane" in a split tab.
    pub fn set_split(&self, split: bool) {
        if self.split.replace(split) != split
            && let Some(c) = &self.close
        {
            c.set_text(if split { "Close Pane" } else { "Close Tab" });
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
