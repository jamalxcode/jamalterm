//! The command palette (⇧⌘P): type a few letters of any command, setting,
//! theme or tab and press ⏎. Letters match in order, not necessarily
//! together ("spr" finds "Split Pane Right"); starts of words count most.
//! What you ran recently comes first when nothing is typed.

use crate::commands::{self, Cmd};
use crate::settings::Settings;

/// An on/off setting that works in this version: its label, where it lives
/// in Settings, whether it's under Advanced, and the field. The Settings
/// window and the palette are both built from this list.
pub struct Toggle {
    pub label: &'static str,
    pub pane: Pane,
    pub advanced: bool,
    pub field: fn(&mut Settings) -> &mut bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Pane {
    #[default]
    Appearance,
    Shell,
    Ai,
    Privacy,
}

impl Pane {
    pub const ALL: [Pane; 4] = [Pane::Appearance, Pane::Shell, Pane::Ai, Pane::Privacy];
    pub fn name(self) -> &'static str {
        match self {
            Pane::Appearance => "Appearance",
            Pane::Shell => "Shell",
            Pane::Ai => "AI",
            Pane::Privacy => "Privacy",
        }
    }
}

pub const TOGGLES: &[Toggle] = &[
    Toggle {
        label: "Blinking cursor",
        pane: Pane::Appearance,
        advanced: false,
        field: |s| &mut s.cursor_blink,
    },
    Toggle {
        label: "Always show the tab bar",
        pane: Pane::Appearance,
        advanced: false,
        field: |s| &mut s.show_tab_bar,
    },
    Toggle {
        label: "↑ at an empty prompt browses past commands",
        pane: Pane::Shell,
        advanced: false,
        field: |s| &mut s.arrow_browse,
    },
    Toggle {
        label: "Ask before re-running a command in another folder",
        pane: Pane::Shell,
        advanced: false,
        field: |s| &mut s.confirm_rerun,
    },
    Toggle {
        label: "Reopen windows and tabs after a restart",
        pane: Pane::Shell,
        advanced: false,
        field: |s| &mut s.restore_session,
    },
    Toggle {
        label: "Option key as Meta",
        pane: Pane::Shell,
        advanced: false,
        field: |s| &mut s.option_is_meta,
    },
    Toggle {
        label: "AI features (the AI panel, ⌘I)",
        pane: Pane::Ai,
        advanced: false,
        field: |s| &mut s.ai_enabled,
    },
    Toggle {
        label: "Send command output with AI questions (secrets are hidden first)",
        pane: Pane::Privacy,
        advanced: false,
        field: |s| &mut s.ai_share_output,
    },
    Toggle {
        label: "Shell integration (marks, browse mode) for new tabs",
        pane: Pane::Shell,
        advanced: true,
        field: |s| &mut s.shell_integration,
    },
];

#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    Cmd(Cmd),
    /// Flip TOGGLES[i].
    Toggle(usize),
    Theme(&'static str),
    /// Show a tab: (window id, its place).
    Tab(u64, usize),
    /// Open Settings at a pane.
    Settings(Pane),
    /// Use this AI assistant.
    Provider(&'static str),
}

#[derive(Clone, Debug)]
pub struct Item {
    pub title: String,
    /// "Shell", "Setting", "Theme", "Tab"…
    pub group: &'static str,
    /// The shortcut, as the menu shows it.
    pub keys: String,
    pub action: Action,
}

/// "⇧⌘P" on a Mac, "Ctrl+Shift+P" elsewhere.
pub fn key_label(e: &commands::Entry) -> String {
    let Some(k) = e.key else {
        return String::new();
    };
    let name = match k {
        egui::Key::ArrowUp => "↑".to_string(),
        egui::Key::ArrowDown => "↓".into(),
        egui::Key::ArrowLeft => "←".into(),
        egui::Key::ArrowRight => "→".into(),
        egui::Key::Equals => "=".into(),
        egui::Key::Minus => "-".into(),
        egui::Key::Comma => ",".into(),
        egui::Key::OpenBracket => "[".into(),
        egui::Key::CloseBracket => "]".into(),
        k => k.name().to_string(),
    };
    if cfg!(target_os = "macos") {
        format!(
            "{}{}⌘{name}",
            if e.alt { "⌥" } else { "" },
            if e.shift { "⇧" } else { "" }
        )
    } else {
        format!("Ctrl+Shift+{}{name}", if e.alt { "Alt+" } else { "" })
    }
}

/// Everything the palette offers (tabs are added by the app).
pub fn items(settings: &Settings) -> Vec<Item> {
    let mut v = vec![];
    for e in commands::TABLE.iter().filter(|e| e.menu.is_some()) {
        if e.cmd == Cmd::Palette {
            continue;
        }
        v.push(Item {
            title: e.title.trim_end_matches('…').to_string(),
            group: match e.menu {
                Some(commands::Menu::App) => "jamalterm",
                Some(commands::Menu::Shell) => "Shell",
                Some(commands::Menu::Edit) => "Edit",
                Some(commands::Menu::View) => "View",
                _ => "Window",
            },
            keys: key_label(e),
            action: Action::Cmd(e.cmd),
        });
    }
    let mut s = settings.clone();
    for (i, t) in TOGGLES.iter().enumerate() {
        let on = *(t.field)(&mut s);
        v.push(Item {
            title: format!("{}: turn {}", t.label, if on { "off" } else { "on" }),
            group: "Setting",
            keys: String::new(),
            action: Action::Toggle(i),
        });
    }
    for t in crate::themes::themes() {
        v.push(Item {
            title: format!("Theme: {}", t.name),
            group: "Theme",
            keys: if t.id == settings.theme {
                "current".into()
            } else {
                String::new()
            },
            action: Action::Theme(t.id),
        });
    }
    for p in crate::ai::PROVIDERS {
        v.push(Item {
            title: format!("AI assistant: {}", p.label),
            group: "AI",
            keys: if p.id == settings.ai_provider {
                "current".into()
            } else {
                String::new()
            },
            action: Action::Provider(p.id),
        });
    }
    for p in Pane::ALL {
        v.push(Item {
            title: format!("Settings: {}", p.name()),
            group: "Settings",
            keys: String::new(),
            action: Action::Settings(p),
        });
    }
    for (what, p) in [
        ("Font size", Pane::Appearance),
        ("Cursor style", Pane::Appearance),
        ("Line height", Pane::Appearance),
        ("Shell program", Pane::Shell),
        ("Scrollback lines", Pane::Shell),
        ("Prompt keys (emacs or vi)", Pane::Shell),
    ] {
        v.push(Item {
            title: format!("{what}…"),
            group: "Setting",
            keys: String::new(),
            action: Action::Settings(p),
        });
    }
    v
}

/// How well `query` matches `text` (None: not at all). Letters must appear
/// in order; a letter at the start of a word or right after the previous
/// match scores more; case doesn't matter.
pub fn score(query: &str, text: &str) -> Option<i32> {
    let q: Vec<char> = query
        .to_lowercase()
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    if q.is_empty() {
        return Some(0);
    }
    let t: Vec<char> = text.to_lowercase().chars().collect();
    let mut qi = 0;
    let mut total = 0;
    let mut last: Option<usize> = None;
    for (i, c) in t.iter().enumerate() {
        if qi < q.len() && *c == q[qi] {
            let word_start = i == 0 || !t[i - 1].is_alphanumeric();
            total += 1;
            if word_start {
                total += 8;
            }
            if last == Some(i.wrapping_sub(1)) {
                total += 4;
            }
            last = Some(i);
            qi += 1;
        }
    }
    (qi == q.len()).then(|| total * 100 / (t.len() as i32 + 10))
}

/// The items matching `query`, best first; with nothing typed, the recent
/// ones first, then the rest in order.
pub fn filter<'a>(items: &'a [Item], query: &str, recent: &[Action]) -> Vec<&'a Item> {
    if query.trim().is_empty() {
        let mut v: Vec<&Item> = recent
            .iter()
            .filter_map(|a| items.iter().find(|i| i.action == *a))
            .collect();
        v.extend(items.iter().filter(|i| !recent.contains(&i.action)));
        return v;
    }
    let mut v: Vec<(i32, usize, &Item)> = items
        .iter()
        .enumerate()
        .filter_map(|(n, i)| score(query, &i.title).map(|s| (s, n, i)))
        .collect();
    v.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    v.into_iter().map(|(_, _, i)| i).collect()
}

/// The palette while it's open.
#[derive(Default)]
pub struct Palette {
    pub query: String,
    pub selected: usize,
    pub focus: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matching() {
        assert!(score("spr", "Split Pane Right").is_some());
        assert!(score("rps", "Split Pane Right").is_none(), "in order");
        assert!(score("SPLIT", "Split Pane Right").is_some(), "any case");
        assert!(score("", "anything") == Some(0));
        // Word starts beat letters in the middle.
        assert!(score("nt", "New Tab").unwrap() > score("nt", "Find Next").unwrap());
    }

    #[test]
    fn the_list() {
        let s = Settings::default();
        let all = items(&s);
        let titles: Vec<&str> = all.iter().map(|i| i.title.as_str()).collect();
        assert!(titles.contains(&"Split Pane Right"));
        assert_eq!(
            titles.iter().filter(|t| t.starts_with("Theme: ")).count(),
            10
        );
        assert!(
            !titles.iter().any(|t| t.starts_with("Command Palette")),
            "not itself"
        );
        assert!(
            all.iter()
                .any(|i| i.group == "Setting" && i.title.contains("Blinking cursor"))
        );
        let best = filter(&all, "split right", &[]);
        assert_eq!(best[0].title, "Split Pane Right");
        let nt = filter(&all, "new tab", &[]);
        assert_eq!(nt[0].title, "New Tab");
        assert!(!nt[0].keys.is_empty(), "shows its shortcut");
        // Recent first when nothing is typed.
        let recent = [Action::Cmd(Cmd::MergeAllWindows)];
        assert_eq!(filter(&all, "", &recent)[0].title, "Merge All Windows");
    }

    #[test]
    fn toggles_flip_their_field() {
        let mut s = Settings::default();
        for t in TOGGLES {
            let before = *(t.field)(&mut s);
            *(t.field)(&mut s) = !before;
            assert_ne!(*(t.field)(&mut s), before, "{}", t.label);
        }
    }
}
