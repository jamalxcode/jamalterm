//! Every app command in one table: its menu title, which menu it's in and its
//! shortcut (⌘ on a Mac, Ctrl+Shift elsewhere). The keyboard and the Mac menu
//! bar both look commands up here, so a shortcut and its menu item can't
//! drift apart.

// Menu titles and ids are only read by the Mac menu bar.
#![cfg_attr(not(target_os = "macos"), allow(dead_code))]

use egui::Key;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Cmd {
    NewTab,
    SplitRight,
    SplitDown,
    /// ⌘W: the focused pane, or the tab when it isn't split.
    ClosePane,
    /// ⌥⌘W: the tab with all its panes.
    CloseTab,
    Copy,
    Paste,
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
    /// ⌘1–⌘9 (a key only: the menu lists no tabs that aren't open).
    Tab(u8),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Menu {
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
    pub key: Key,
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
        key,
        shift,
        alt: false,
        keyed: true,
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
        key,
        shift: false,
        alt: false,
        keyed: true,
    }
}

pub const TABLE: &[Entry] = &[
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
    Entry {
        keyed: false,
        ..e(Cmd::Copy, "Copy", Menu::Edit, Key::C, false)
    },
    Entry {
        keyed: false,
        ..e(Cmd::Paste, "Paste", Menu::Edit, Key::V, false)
    },
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
    e(Cmd::Clear, "Clear Scrollback", Menu::Edit, Key::K, false),
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
        .filter(|e| e.keyed && e.key == key && e.alt == alt)
        .find(|e| shift.is_none_or(|s| e.shift == s || e.key == Key::Equals))
        .map(|e| e.cmd)
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
        assert_eq!(for_key(Key::CloseBracket, Some(false), false), None);
        assert_eq!(for_key(Key::Num3, Some(false), false), Some(Cmd::Tab(3)));
        assert_eq!(
            for_key(Key::C, Some(false), false),
            None,
            "Copy is the system's"
        );
        assert_eq!(for_key(Key::Q, Some(false), false), None);
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
    fn every_shortcut_is_unique_and_ids_round_trip() {
        for (i, a) in TABLE.iter().enumerate() {
            assert_eq!(by_id(&id(a.cmd)), Some(a.cmd));
            for b in &TABLE[i + 1..] {
                assert!(
                    !(a.key == b.key && a.shift == b.shift && a.alt == b.alt),
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
