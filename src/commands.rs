//! Every app command in one table: its menu title, which menu it's in and its
//! shortcut (⌘ on a Mac, Ctrl+Shift elsewhere). The keyboard and the Mac menu
//! bar both look commands up here, so a shortcut and its menu item can't
//! drift apart.

// Menu titles and ids are only read by the Mac menu bar.
#![cfg_attr(not(target_os = "macos"), allow(dead_code))]

use egui::Key;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Cmd {
    Settings,
    /// ⌘Q: asks first when programs are running.
    Quit,
    /// Right-click → Search with Google (the selected text).
    SearchWeb,
    Palette,
    /// ⌘I: show or hide the AI panel.
    AiPanel,
    NewWindow,
    NewTab,
    SplitRight,
    SplitDown,
    /// ⌘W: the focused pane, or the tab when it isn't split.
    ClosePane,
    /// ⌥⌘W: the tab with all its panes.
    CloseTab,
    CloseWindow,
    Copy,
    Paste,
    SelectAll,
    /// ⌘E: the selected text becomes what Find looks for.
    FindSelection,
    ToggleTabBar,
    Find,
    FindNext,
    FindPrevious,
    Browse,
    Clear,
    Bigger,
    Smaller,
    ActualSize,
    PreviousTab,
    NextTab,
    PreviousPane,
    NextPane,
    PaneLeft,
    PaneRight,
    PaneUp,
    PaneDown,
    MoveTabToNewWindow,
    MergeAllWindows,
    /// ⌘1–⌘9 (a key only: the menu lists no tabs that aren't open).
    Tab(u8),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Menu {
    /// The jamalterm menu (Settings…).
    App,
    Shell,
    Edit,
    View,
    Window,
}

pub struct Entry {
    pub cmd: Cmd,
    /// The menu item's text: what it will do.
    pub title: &'static str,
    pub menu: Option<Menu>,
    /// None: a menu item with no shortcut.
    pub key: Option<Key>,
    /// ⇧ as well as ⌘ (on a Mac; elsewhere ⇧ is always part of the shortcut).
    pub shift: bool,
    /// ⌥ as well.
    pub alt: bool,
    /// The keyboard looks this one up (Copy and Paste come as their own
    /// events from the system, so they're only menu items).
    pub keyed: bool,
}

const fn e(cmd: Cmd, title: &'static str, menu: Menu, key: Key, shift: bool) -> Entry {
    Entry {
        cmd,
        title,
        menu: Some(menu),
        key: Some(key),
        shift,
        alt: false,
        keyed: true,
    }
}

/// A menu item with no shortcut.
const fn item(cmd: Cmd, title: &'static str, menu: Menu) -> Entry {
    Entry {
        key: None,
        keyed: false,
        ..e(cmd, title, menu, Key::Escape, false)
    }
}

/// An entry whose shortcut has ⌥ too.
const fn alt(cmd: Cmd, title: &'static str, menu: Menu, key: Key) -> Entry {
    Entry {
        alt: true,
        ..e(cmd, title, menu, key, false)
    }
}

const fn tab(n: u8, key: Key) -> Entry {
    Entry {
        cmd: Cmd::Tab(n),
        title: "",
        menu: None,
        key: Some(key),
        shift: false,
        alt: false,
        keyed: true,
    }
}

pub const TABLE: &[Entry] = &[
    e(Cmd::Settings, "Settings…", Menu::App, Key::Comma, false),
    Entry {
        menu: None, // the jamalterm menu has its own Quit item, at the end
        ..e(Cmd::Quit, "Quit jamalterm", Menu::App, Key::Q, false)
    },
    Entry {
        menu: None,
        key: None,
        keyed: false,
        ..e(
            Cmd::SearchWeb,
            "Search with Google",
            Menu::Edit,
            Key::Escape,
            false,
        )
    },
    e(Cmd::NewWindow, "New Window", Menu::Shell, Key::N, false),
    e(Cmd::NewTab, "New Tab", Menu::Shell, Key::T, false),
    e(
        Cmd::SplitRight,
        "Split Pane Right",
        Menu::Shell,
        Key::D,
        false,
    ),
    e(Cmd::SplitDown, "Split Pane Down", Menu::Shell, Key::D, true),
    // "Close Pane" while the tab is split (the menu says what it'll do).
    e(Cmd::ClosePane, "Close Tab", Menu::Shell, Key::W, false),
    alt(
        Cmd::CloseTab,
        "Close Tab and Its Panes",
        Menu::Shell,
        Key::W,
    ),
    e(Cmd::CloseWindow, "Close Window", Menu::Shell, Key::W, true),
    Entry {
        keyed: false,
        ..e(Cmd::Copy, "Copy", Menu::Edit, Key::C, false)
    },
    Entry {
        keyed: false,
        ..e(Cmd::Paste, "Paste", Menu::Edit, Key::V, false)
    },
    e(Cmd::SelectAll, "Select All", Menu::Edit, Key::A, false),
    e(Cmd::Find, "Find…", Menu::Edit, Key::F, false),
    e(Cmd::FindNext, "Find Next", Menu::Edit, Key::G, false),
    e(Cmd::FindPrevious, "Find Previous", Menu::Edit, Key::G, true),
    e(
        Cmd::Browse,
        "Browse Past Commands",
        Menu::Edit,
        Key::ArrowUp,
        false,
    ),
    e(
        Cmd::FindSelection,
        "Use Selection for Find",
        Menu::Edit,
        Key::E,
        false,
    ),
    e(Cmd::Clear, "Clear Scrollback", Menu::Edit, Key::K, false),
    e(Cmd::Palette, "Command Palette…", Menu::View, Key::P, true),
    e(Cmd::AiPanel, "Show AI Panel", Menu::View, Key::I, false),
    // "Hide Tab Bar" while it's shown (the menu says what it'll do).
    e(Cmd::ToggleTabBar, "Show Tab Bar", Menu::View, Key::T, true),
    e(Cmd::Bigger, "Bigger", Menu::View, Key::Equals, false),
    e(Cmd::Smaller, "Smaller", Menu::View, Key::Minus, false),
    e(Cmd::ActualSize, "Actual Size", Menu::View, Key::Num0, false),
    e(
        Cmd::PreviousTab,
        "Show Previous Tab",
        Menu::Window,
        Key::OpenBracket,
        true,
    ),
    e(
        Cmd::NextTab,
        "Show Next Tab",
        Menu::Window,
        Key::CloseBracket,
        true,
    ),
    e(
        Cmd::PreviousPane,
        "Select Previous Pane",
        Menu::Window,
        Key::OpenBracket,
        false,
    ),
    e(
        Cmd::NextPane,
        "Select Next Pane",
        Menu::Window,
        Key::CloseBracket,
        false,
    ),
    alt(
        Cmd::PaneLeft,
        "Select Pane on the Left",
        Menu::Window,
        Key::ArrowLeft,
    ),
    alt(
        Cmd::PaneRight,
        "Select Pane on the Right",
        Menu::Window,
        Key::ArrowRight,
    ),
    alt(Cmd::PaneUp, "Select Pane Above", Menu::Window, Key::ArrowUp),
    alt(
        Cmd::PaneDown,
        "Select Pane Below",
        Menu::Window,
        Key::ArrowDown,
    ),
    item(
        Cmd::MoveTabToNewWindow,
        "Move Tab to New Window",
        Menu::Window,
    ),
    item(Cmd::MergeAllWindows, "Merge All Windows", Menu::Window),
    tab(1, Key::Num1),
    tab(2, Key::Num2),
    tab(3, Key::Num3),
    tab(4, Key::Num4),
    tab(5, Key::Num5),
    tab(6, Key::Num6),
    tab(7, Key::Num7),
    tab(8, Key::Num8),
    tab(9, Key::Num9),
];

/// The command for a shortcut key. `shift` is None where ⇧ is always held
/// (Ctrl+Shift on Linux): the first entry for the key wins there.
pub fn for_key(key: Key, shift: Option<bool>, alt: bool) -> Option<Cmd> {
    // ⌘+ (⇧⌘=) is Bigger too, as in every Mac app.
    let key = if key == Key::Plus { Key::Equals } else { key };
    TABLE
        .iter()
        .filter(|e| e.keyed && e.key == Some(key) && e.alt == alt)
        .find(|e| shift.is_none_or(|s| e.shift == s || key == Key::Equals))
        .map(|e| e.cmd)
}

/// The right-click menu: (command, title, enabled), None for a separator.
pub fn context_menu(selection: bool) -> Vec<Option<(Cmd, &'static str, bool)>> {
    vec![
        Some((Cmd::Copy, "Copy", selection)),
        Some((Cmd::Paste, "Paste", true)),
        Some((Cmd::SelectAll, "Select All", true)),
        None,
        Some((Cmd::FindSelection, "Find Selection", selection)),
        Some((Cmd::SearchWeb, "Search with Google", selection)),
        None,
        Some((Cmd::SplitRight, "Split Pane Right", true)),
        Some((Cmd::SplitDown, "Split Pane Down", true)),
        None,
        Some((Cmd::Clear, "Clear Scrollback", true)),
    ]
}

/// A stable id for the menu item (muda's MenuId).
pub fn id(cmd: Cmd) -> String {
    format!("{cmd:?}")
}

pub fn by_id(id: &str) -> Option<Cmd> {
    TABLE.iter().map(|e| e.cmd).find(|c| self::id(*c) == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_find_their_commands() {
        assert_eq!(for_key(Key::T, Some(false), false), Some(Cmd::NewTab));
        assert_eq!(for_key(Key::G, Some(false), false), Some(Cmd::FindNext));
        assert_eq!(for_key(Key::G, Some(true), false), Some(Cmd::FindPrevious));
        assert_eq!(
            for_key(Key::G, None, false),
            Some(Cmd::FindNext),
            "Linux: ⇧ held"
        );
        assert_eq!(for_key(Key::Plus, Some(true), false), Some(Cmd::Bigger));
        assert_eq!(for_key(Key::Equals, Some(false), false), Some(Cmd::Bigger));
        assert_eq!(
            for_key(Key::CloseBracket, Some(true), false),
            Some(Cmd::NextTab)
        );
        assert_eq!(for_key(Key::CloseBracket, None, false), Some(Cmd::NextTab));
        assert_eq!(
            for_key(Key::CloseBracket, Some(false), false),
            Some(Cmd::NextPane)
        );
        assert_eq!(for_key(Key::Num3, Some(false), false), Some(Cmd::Tab(3)));
        assert_eq!(
            for_key(Key::C, Some(false), false),
            None,
            "Copy is the system's"
        );
        assert_eq!(for_key(Key::Q, Some(false), false), Some(Cmd::Quit));
        assert_eq!(for_key(Key::Z, Some(false), false), None);
        assert_eq!(for_key(Key::N, Some(false), false), Some(Cmd::NewWindow));
        assert_eq!(for_key(Key::W, Some(true), false), Some(Cmd::CloseWindow));
        assert_eq!(
            for_key(Key::Escape, Some(false), false),
            None,
            "no shortcut"
        );
        assert_eq!(for_key(Key::W, Some(false), false), Some(Cmd::ClosePane));
        assert_eq!(for_key(Key::W, Some(false), true), Some(Cmd::CloseTab));
        assert_eq!(for_key(Key::ArrowUp, Some(false), false), Some(Cmd::Browse));
        assert_eq!(for_key(Key::ArrowUp, Some(false), true), Some(Cmd::PaneUp));
        assert_eq!(for_key(Key::D, Some(true), false), Some(Cmd::SplitDown));
        assert_eq!(
            for_key(Key::OpenBracket, Some(false), false),
            Some(Cmd::PreviousPane)
        );
    }

    #[test]
    fn right_click_menu() {
        let m = context_menu(false);
        let copy = m.iter().flatten().find(|i| i.0 == Cmd::Copy).unwrap();
        assert!(!copy.2, "Copy is off with nothing selected");
        assert!(context_menu(true).iter().flatten().all(|i| i.2));
        for (cmd, ..) in m.iter().flatten() {
            assert_eq!(by_id(&id(*cmd)), Some(*cmd), "its clicks come back");
        }
    }

    #[test]
    fn every_shortcut_is_unique_and_ids_round_trip() {
        for (i, a) in TABLE.iter().enumerate() {
            assert_eq!(by_id(&id(a.cmd)), Some(a.cmd));
            for b in &TABLE[i + 1..] {
                assert!(
                    a.key.is_none() || !(a.key == b.key && a.shift == b.shift && a.alt == b.alt),
                    "{:?} and {:?} share a shortcut",
                    a.cmd,
                    b.cmd
                );
            }
            if a.menu.is_some() {
                assert!(!a.title.is_empty(), "{:?} needs a menu title", a.cmd);
            }
        }
    }
}
