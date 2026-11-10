//! The Mac menu bar (jamalterm, Shell, Edit, View, Window), built from the
//! command table. A menu item's shortcut is a real key equivalent, so the
//! menu title flashes when it's pressed, as in any Mac app. Clicks and key
//! equivalents arrive as command ids on a channel the app reads each frame.

use crate::commands::{self, Cmd, Menu};
use muda::accelerator::{Accelerator, Code, Modifiers};
use muda::{
    AboutMetadata, CheckMenuItem, IsMenuItem, MenuEvent, MenuItem, PredefinedMenuItem, Submenu,
};
use std::cell::RefCell;
use std::sync::mpsc;

pub struct MacMenu {
    _bar: muda::Menu,
    rx: mpsc::Receiver<Cmd>,
    /// Every command's item, to retitle ("Close Pane", "Hide Tab Bar"…).
    by_cmd: Vec<(Cmd, MenuItem, RefCell<String>)>,
    /// jamalterm → Secure Keyboard Entry (a checkmark).
    secure: CheckMenuItem,
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
        K::O => Code::KeyO,
        K::I => Code::KeyI,
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
            .filter(|e| e.cmd != Cmd::SecureInput)
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
            version: Some(crate::update::current_version().into()),
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
            // Our own Quit (⌘Q), so it can ask when programs are running.
            item(commands::TABLE.iter().find(|e| e.cmd == Cmd::Quit)?),
        );
        let secure = CheckMenuItem::with_id(
            commands::id(Cmd::SecureInput),
            "Secure Keyboard Entry",
            true,
            false,
            None,
        );
        let mut app_items: Vec<&dyn IsMenuItem> = vec![&about, &s0];
        app_items.extend(menu(Menu::App)); // Settings…
        app_items.push(&secure);
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

        let help = Submenu::with_items("Help", true, &menu(Menu::Help)).ok()?;
        let bar = muda::Menu::with_items(&[&app, &shell, &edit, &view, &window, &help]).ok()?;
        bar.init_for_nsapp();
        window.set_as_windows_menu_for_nsapp();
        help.set_as_help_menu_for_nsapp();
        let by_cmd = all
            .iter()
            .map(|(c, _, it)| (*c, it.clone(), RefCell::new(it.text())))
            .collect();
        Some(MacMenu {
            _bar: bar,
            rx,
            by_cmd,
            secure,
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

    /// The checkmark on Secure Keyboard Entry.
    pub fn set_secure(&self, on: bool) {
        if self.secure.is_checked() != on {
            self.secure.set_checked(on);
        }
    }

    /// Commands chosen since the last frame.
    pub fn chosen(&self) -> Vec<Cmd> {
        self.rx.try_iter().collect()
    }
}

/// The right-click menu, native, at the pointer in the window in front.
/// Its clicks come back as commands, like the menu bar's.
pub fn show_context_menu(selection: bool) {
    use muda::ContextMenu;
    use objc2::MainThreadMarker;
    use objc2_app_kit::NSApplication;
    let menu = muda::Menu::new();
    for item in commands::context_menu(selection) {
        let _ = match item {
            Some((cmd, title, enabled)) => {
                menu.append(&MenuItem::with_id(commands::id(cmd), title, enabled, None))
            }
            None => menu.append(&PredefinedMenuItem::separator()),
        };
    }
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let app = NSApplication::sharedApplication(mtm);
    let Some(view) = app.keyWindow().and_then(|w| w.contentView()) else {
        return;
    };
    // Safety: a live NSView of our own window, on the main thread.
    unsafe {
        menu.show_context_menu_for_nsview(objc2::rc::Retained::as_ptr(&view).cast(), None);
    }
}

// ---- The menu bar icon and the Dock menu ----------------------------------

/// One window for the switcher: its id, its name (the tab in front) and
/// its tabs' names, with the one in front.
pub struct Listed {
    pub id: u64,
    pub name: String,
    pub tabs: Vec<String>,
    pub active: usize,
}

/// The switcher list as a menu: every window and its tabs, then New
/// Window, New Tab, Show All Windows (and, for the icon, Hide This Icon).
fn switcher_menu(list: &[Listed], icon: bool) -> muda::Menu {
    let menu = muda::Menu::new();
    for (n, w) in list.iter().enumerate() {
        let title = if list.len() > 1 {
            format!("Window {}: {}", n + 1, w.name)
        } else {
            w.name.clone()
        };
        let _ = menu.append(&MenuItem::with_id(
            commands::go_id(w.id, w.active),
            title,
            true,
            None,
        ));
        if w.tabs.len() > 1 {
            for (i, t) in w.tabs.iter().enumerate() {
                let mark = if i == w.active { "•" } else { " " };
                let _ = menu.append(&MenuItem::with_id(
                    commands::go_id(w.id, i),
                    format!("    {mark} Tab {}: {t}", i + 1),
                    true,
                    None,
                ));
            }
        }
    }
    let _ = menu.append(&PredefinedMenuItem::separator());
    let _ = menu.append(&MenuItem::with_id(
        commands::id(Cmd::NewWindow),
        "New Window",
        true,
        None,
    ));
    let _ = menu.append(&MenuItem::with_id(
        commands::id(Cmd::NewTab),
        "New Tab",
        true,
        None,
    ));
    let _ = menu.append(&MenuItem::with_id(
        "ShowAll",
        "Show All Windows",
        true,
        None,
    ));
    if icon {
        let _ = menu.append(&PredefinedMenuItem::separator());
        let _ = menu.append(&MenuItem::with_id(
            "HideMenuBarIcon",
            "Hide This Icon",
            true,
            None,
        ));
    }
    menu
}

/// A ">_" in a 36×36 template image (macOS tints it for light and dark
/// menu bars).
fn icon_image() -> Option<tray_icon::Icon> {
    let n = 36usize;
    let mut px = vec![0u8; n * n * 4];
    let mut dot = |x: usize, y: usize| {
        if x < n && y < n {
            let i = (y * n + x) * 4;
            px[i + 3] = 255;
        }
    };
    // The chevron.
    for t in 0..9 {
        for w in 0..3 {
            dot(7 + t + w, 9 + t);
            dot(7 + t + w, 27 - t);
        }
    }
    // The underscore.
    for x in 18..30 {
        for y in 24..27 {
            dot(x, y);
        }
    }
    tray_icon::Icon::from_rgba(px, n as u32, n as u32).ok()
}

thread_local! {
    /// The Dock menu macOS asks for (main thread only).
    static DOCK: std::cell::RefCell<Option<muda::Menu>> = const { std::cell::RefCell::new(None) };
}

/// AppKit asks the app's delegate for the Dock menu: winit's delegate has
/// no such method, so one is added that returns the switcher menu.
fn install_dock_menu() {
    use objc2::runtime::{AnyClass, AnyObject, Sel};
    use objc2::{MainThreadMarker, sel};
    use objc2_app_kit::NSApplication;
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let app = NSApplication::sharedApplication(mtm);
    let Some(delegate) = app.delegate() else {
        return;
    };
    extern "C-unwind" fn dock_menu(
        _this: &AnyObject,
        _cmd: Sel,
        _app: *mut AnyObject,
    ) -> *mut AnyObject {
        use muda::ContextMenu;
        DOCK.with(|d| match &*d.borrow() {
            Some(m) => m.ns_menu() as *mut AnyObject,
            None => std::ptr::null_mut(),
        })
    }
    let obj: &AnyObject = objc2::runtime::ProtocolObject::as_ref(&*delegate);
    let cls: *const AnyClass = obj.class();
    unsafe {
        let imp: objc2::runtime::Imp = std::mem::transmute(
            dock_menu as extern "C-unwind" fn(&AnyObject, Sel, *mut AnyObject) -> *mut AnyObject,
        );
        objc2::ffi::class_addMethod(
            cls as *mut AnyClass,
            sel!(applicationDockMenu:),
            imp,
            c"@@:@".as_ptr(),
        );
    }
}

/// The menu bar icon and the Dock menu, rebuilt when windows or tabs change.
pub struct Switcher {
    tray: Option<tray_icon::TrayIcon>,
    last: String,
    dock_ready: bool,
}

impl Switcher {
    pub fn new() -> Switcher {
        Switcher {
            tray: None,
            last: String::new(),
            dock_ready: false,
        }
    }

    pub fn update(&mut self, list: &[Listed], show_icon: bool) {
        if !self.dock_ready {
            install_dock_menu();
            self.dock_ready = true;
        }
        let sig = format!(
            "{show_icon}|{}",
            list.iter()
                .map(|w| format!("{}:{}:{}:{}", w.id, w.name, w.active, w.tabs.join("\u{1}")))
                .collect::<Vec<_>>()
                .join("|")
        );
        if sig == self.last {
            return;
        }
        self.last = sig;
        DOCK.with(|d| *d.borrow_mut() = Some(switcher_menu(list, false)));
        if !show_icon {
            self.tray = None; // dropping it removes the icon
            return;
        }
        let menu = Box::new(switcher_menu(list, true));
        match &self.tray {
            Some(t) => t.set_menu(Some(menu)),
            None => {
                self.tray = tray_icon::TrayIconBuilder::new()
                    .with_menu(menu)
                    .with_tooltip("jamalterm")
                    .with_icon_templated(match icon_image() {
                        Some(i) => i,
                        None => return,
                    })
                    .build()
                    .ok();
            }
        }
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
