//! Browse mode: a free-floating cursor for moving around past output and
//! re-running commands (3.x's signature feature, DESIGN.md §3). This file is
//! the logic only: keys in, actions out, so it's tested without a window.
//!
//! Lines are absolute (history + screen line, as in `session::Block`).

use crate::session::Block;
use egui::{Key, Modifiers};

#[derive(Debug, Clone, PartialEq)]
pub struct Browse {
    pub row: i64,
    pub col: usize,
    /// Waiting for "run it again?" (a different folder, or Settings asks).
    pub confirm: Option<usize>,
    /// Show every key in the key bar (? toggles).
    pub all_keys: bool,
}

/// What the browse cursor is looking at.
pub struct View<'a> {
    pub blocks: &'a [Block],
    /// The line the shell's cursor is on (the bottom: browsing can't go past it).
    pub bottom: i64,
    pub cols: usize,
    /// The folder the shell is in now.
    pub cwd: &'a str,
    /// Settings → Ask before re-running a command.
    pub confirm_rerun: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    /// Stay in browse mode (the cursor may have moved).
    Stay,
    /// Back to the prompt.
    Exit,
    /// Back to the prompt, and let the key go to the shell (typing).
    ExitAndPass,
    /// Put the command on the prompt and run it (in `cd`'s folder if given).
    Run { command: String, cd: Option<String> },
    /// Put the command on the prompt to edit first.
    Edit(String),
    /// Copy the command (or the line, outside commands).
    CopyCommand,
    /// Copy the output of the command under the cursor.
    CopyOutput,
    /// Ask the AI to fix the (failed) command under the cursor.
    FixIt { command: String, exit: i32 },
    /// Quick Look the path under the cursor (a word like `src/main.rs:12`).
    QuickLook,
}

/// The command whose lines include `row` (prompt through output).
pub fn block_at(blocks: &[Block], row: i64, bottom: i64) -> Option<usize> {
    blocks.iter().enumerate().rev().find_map(|(i, b)| {
        let end = b.end.map(|e| (e - 1).max(b.prompt)).unwrap_or(bottom);
        (row >= b.prompt && row <= end).then_some(i)
    })
}

/// The last line of a block (inclusive).
pub fn block_end(b: &Block, bottom: i64) -> i64 {
    b.end.map(|e| (e - 1).max(b.prompt)).unwrap_or(bottom)
}

impl Browse {
    pub fn new(row: i64, col: usize) -> Self {
        Browse {
            row,
            col,
            confirm: None,
            all_keys: false,
        }
    }

    fn move_to(&mut self, row: i64, v: &View) -> Action {
        if row > v.bottom {
            return Action::Exit; // ↓ past the bottom: back to the prompt
        }
        self.row = row.max(0);
        self.confirm = None;
        Action::Stay
    }

    /// The prompt line of the previous (dir -1) or next (dir 1) command.
    fn jump(&mut self, dir: i64, v: &View) -> Action {
        let target = if dir < 0 {
            v.blocks
                .iter()
                .rev()
                .map(|b| b.prompt)
                .find(|&p| p < self.row)
        } else {
            v.blocks.iter().map(|b| b.prompt).find(|&p| p > self.row)
        };
        match target {
            Some(p) => self.move_to(p, v),
            None if dir > 0 => Action::Exit,
            None => Action::Stay,
        }
    }

    /// Return on a command: run it again, asking first when it ran in
    /// another folder (or Settings says to ask).
    fn run(&mut self, v: &View, alt: bool) -> Action {
        let Some(i) = block_at(v.blocks, self.row, v.bottom) else {
            return Action::Stay;
        };
        let b = &v.blocks[i];
        if b.command.is_empty() {
            return Action::Stay;
        }
        let elsewhere = !b.cwd.is_empty() && !v.cwd.is_empty() && b.cwd != v.cwd;
        if self.confirm == Some(i) {
            // Second Return: here; ⌥Return: in the folder it ran in.
            return Action::Run {
                command: b.command.clone(),
                cd: (alt && elsewhere).then(|| b.cwd.clone()),
            };
        }
        if elsewhere || v.confirm_rerun {
            self.confirm = Some(i);
            return Action::Stay;
        }
        Action::Run {
            command: b.command.clone(),
            cd: None,
        }
    }

    /// A key in browse mode.
    pub fn key(&mut self, key: Key, m: &Modifiers, v: &View) -> Action {
        let jump = m.alt || m.shift;
        match key {
            Key::Escape | Key::Q => Action::Exit,
            Key::ArrowUp | Key::K if jump && key == Key::ArrowUp => self.jump(-1, v),
            Key::ArrowDown | Key::J if jump && key == Key::ArrowDown => self.jump(1, v),
            Key::ArrowUp | Key::K => self.move_to(self.row - 1, v),
            Key::ArrowDown | Key::J => self.move_to(self.row + 1, v),
            Key::ArrowLeft | Key::H => {
                self.col = self.col.saturating_sub(1);
                Action::Stay
            }
            Key::ArrowRight | Key::L => {
                self.col = (self.col + 1).min(v.cols.saturating_sub(1));
                Action::Stay
            }
            Key::Home => {
                self.col = 0;
                Action::Stay
            }
            Key::PageUp => self.move_to(self.row - 20, v),
            Key::PageDown => self.move_to(self.row + 20, v),
            Key::Enter if m.shift => self.edit(v),
            Key::Enter => self.run(v, m.alt),
            Key::E => self.edit(v),
            Key::C => Action::CopyCommand,
            Key::O => Action::CopyOutput,
            Key::F => match block_at(v.blocks, self.row, v.bottom).map(|i| &v.blocks[i]) {
                Some(b) if !b.command.is_empty() && b.exit.is_some_and(|e| e != 0) => {
                    Action::FixIt {
                        command: b.command.clone(),
                        exit: b.exit.unwrap_or(1),
                    }
                }
                _ => Action::Stay,
            },
            Key::Space => Action::QuickLook,
            Key::Questionmark | Key::Slash if m.shift || key == Key::Questionmark => {
                self.all_keys = !self.all_keys;
                Action::Stay
            }
            // Anything else typed goes back to the shell.
            _ => Action::ExitAndPass,
        }
    }

    fn edit(&self, v: &View) -> Action {
        match block_at(v.blocks, self.row, v.bottom).map(|i| &v.blocks[i]) {
            Some(b) if !b.command.is_empty() => Action::Edit(b.command.clone()),
            _ => Action::Stay,
        }
    }

    /// The key bar under the terminal: the everyday keys, or all of them.
    pub fn hint(&self, v: &View) -> (String, Vec<(&'static str, &'static str)>) {
        let block = block_at(v.blocks, self.row, v.bottom).map(|i| &v.blocks[i]);
        if let Some(i) = self.confirm {
            let b = &v.blocks[i];
            if !b.cwd.is_empty() && b.cwd != v.cwd {
                return (
                    format!("⚠ {} ran in {}, you're in {}", b.command, b.cwd, v.cwd),
                    vec![("⏎", "run here"), ("⌥⏎", "run there"), ("esc", "cancel")],
                );
            }
            return (
                format!("Run {} again?", b.command),
                vec![("⏎", "yes"), ("esc", "no")],
            );
        }
        let status = match block {
            Some(b) if !b.command.is_empty() => {
                let state = match b.exit {
                    Some(0) => "✓".to_string(),
                    Some(c) => format!("✕ exit {c}"),
                    None => "… running".to_string(),
                };
                format!("{state}  {}", b.command)
            }
            _ => String::new(),
        };
        let keys = if self.all_keys {
            vec![
                ("↑↓", "move"),
                ("⌥↑↓", "commands"),
                ("⏎", "run"),
                ("⇧⏎", "edit"),
                ("C", "copy"),
                ("O", "copy output"),
                ("F", "fix it (AI)"),
                ("space", "Quick Look"),
                ("esc", "back"),
                ("?", "fewer keys"),
            ]
        } else {
            vec![
                ("⏎", "run"),
                ("⇧⏎", "edit"),
                ("C", "copy"),
                ("esc", "back"),
                ("?", "more keys"),
            ]
        };
        (status, keys)
    }
}

/// The word at a column of a line (spaces and quotes end it), for Quick
/// Look and ⌘-click: "see src/main.rs:12 here" at 6 → "src/main.rs:12".
pub fn word_at(line: &str, col: usize) -> Option<String> {
    let chars: Vec<char> = line.chars().collect();
    let stop = |c: char| c.is_whitespace() || "'\"`()<>[]{},;".contains(c);
    if col >= chars.len() || stop(chars[col]) {
        return None;
    }
    let mut a = col;
    while a > 0 && !stop(chars[a - 1]) {
        a -= 1;
    }
    let mut z = col;
    while z + 1 < chars.len() && !stop(chars[z + 1]) {
        z += 1;
    }
    let w: String = chars[a..=z].iter().collect();
    Some(w.trim_end_matches(['.', ':']).to_string())
}

/// A word as a file path, if it is one: relative paths from `cwd`, `~`,
/// and a `:line` (`:line:col`) suffix dropped.
pub fn as_path(word: &str, cwd: &str, home: &str) -> Option<std::path::PathBuf> {
    let mut w = word.to_string();
    // main.go:12 / main.go:12:3
    while let Some((head, tail)) = w.rsplit_once(':') {
        if !tail.is_empty() && tail.chars().all(|c| c.is_ascii_digit()) {
            w = head.to_string();
        } else {
            break;
        }
    }
    let p = if let Some(rest) = w.strip_prefix("~/") {
        std::path::Path::new(home).join(rest)
    } else if w.starts_with('/') {
        std::path::PathBuf::from(&w)
    } else {
        std::path::Path::new(cwd).join(&w)
    };
    p.exists().then_some(p)
}

/// What typing a command onto the prompt sends: clear the line first (emacs
/// keys: Ctrl+E Ctrl+U; vi keys: Esc, then cc), then the command, then Return
/// to run it. A folder means "cd there first".
pub fn type_command(command: &str, cd: Option<&str>, run: bool, vi_keys: bool) -> Vec<u8> {
    let clear: &[u8] = if vi_keys { b"\x1bcc" } else { b"\x05\x15" };
    let mut out = clear.to_vec();
    if let Some(dir) = cd {
        out.extend(format!("cd {} && ", shell_quote(dir)).as_bytes());
    }
    out.extend(command.as_bytes());
    if run {
        out.push(b'\r');
    }
    out
}

fn shell_quote(s: &str) -> String {
    if s.chars()
        .all(|c| c.is_ascii_alphanumeric() || "/._-~".contains(c))
    {
        s.to_string()
    } else {
        format!("'{}'", s.replace('\'', r"'\''"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    #[test]
    fn words_and_paths_under_the_cursor() {
        let line = "error in src/main.rs:12:3 (see 'notes.txt').";
        assert_eq!(word_at(line, 12).as_deref(), Some("src/main.rs:12:3"));
        assert_eq!(word_at(line, 33).as_deref(), Some("notes.txt"));
        assert_eq!(word_at(line, 5), None, "a space");
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        std::fs::write(dir.path().join("src/main.rs"), "").unwrap();
        let cwd = dir.path().display().to_string();
        assert_eq!(
            as_path("src/main.rs:12:3", &cwd, "/nowhere"),
            Some(dir.path().join("src/main.rs"))
        );
        assert_eq!(
            as_path("~/src/main.rs", "/", &cwd),
            Some(dir.path().join("src/main.rs"))
        );
        assert_eq!(as_path("missing.txt", &cwd, "/"), None);
    }

    fn block(prompt: i64, end: Option<i64>, cmd: &str, cwd: &str, exit: Option<i32>) -> Block {
        Block {
            prompt,
            start: Some(prompt + 1),
            end,
            command: cmd.into(),
            exit,
            started_at: Some(Instant::now()),
            finished_at: None,
            cwd: cwd.into(),
        }
    }

    fn blocks() -> Vec<Block> {
        vec![
            block(0, Some(3), "ls", "/home", Some(0)),
            block(3, Some(5), "make", "/srv", Some(2)),
            block(5, Some(6), "pwd", "/home", Some(0)),
        ]
    }

    fn view(b: &[Block]) -> View<'_> {
        View {
            blocks: b,
            bottom: 6,
            cols: 80,
            cwd: "/home",
            confirm_rerun: false,
        }
    }

    #[test]
    fn moving_and_leaving() {
        let b = blocks();
        let v = view(&b);
        let mut br = Browse::new(6, 0);
        assert_eq!(br.key(Key::ArrowUp, &Modifiers::NONE, &v), Action::Stay);
        assert_eq!(br.row, 5);
        let alt = Modifiers {
            alt: true,
            ..Modifiers::NONE
        };
        br.key(Key::ArrowUp, &alt, &v);
        assert_eq!(br.row, 3, "⌥↑: the previous command");
        br.key(Key::ArrowUp, &alt, &v);
        assert_eq!(br.row, 0);
        br.key(Key::ArrowDown, &alt, &v);
        assert_eq!(br.row, 3, "⌥↓: the next command");
        let mut br = Browse::new(6, 0);
        assert_eq!(
            br.key(Key::ArrowDown, &Modifiers::NONE, &v),
            Action::Exit,
            "↓ past the bottom"
        );
        assert_eq!(br.key(Key::Escape, &Modifiers::NONE, &v), Action::Exit);
        assert_eq!(
            br.key(Key::X, &Modifiers::NONE, &v),
            Action::ExitAndPass,
            "typing goes to the shell"
        );
    }

    #[test]
    fn rerun_asks_when_the_folder_differs() {
        let b = blocks();
        let v = view(&b);
        let mut br = Browse::new(1, 0);
        assert_eq!(
            br.key(Key::Enter, &Modifiers::NONE, &v),
            Action::Run {
                command: "ls".into(),
                cd: None
            },
            "same folder: runs"
        );
        let mut br = Browse::new(4, 0);
        assert_eq!(
            br.key(Key::Enter, &Modifiers::NONE, &v),
            Action::Stay,
            "make ran in /srv: asks first"
        );
        assert!(br.hint(&v).0.contains("ran in /srv, you're in /home"));
        let alt = Modifiers {
            alt: true,
            ..Modifiers::NONE
        };
        assert_eq!(
            br.key(Key::Enter, &alt, &v),
            Action::Run {
                command: "make".into(),
                cd: Some("/srv".into())
            },
            "⌥⏎: in its folder"
        );
        let shift = Modifiers {
            shift: true,
            ..Modifiers::NONE
        };
        assert_eq!(br.key(Key::Enter, &shift, &v), Action::Edit("make".into()));
        let ask = View {
            confirm_rerun: true,
            ..view(&b)
        };
        let mut br = Browse::new(1, 0);
        assert_eq!(
            br.key(Key::Enter, &Modifiers::NONE, &ask),
            Action::Stay,
            "Settings: always ask"
        );
        assert_eq!(
            br.key(Key::Enter, &Modifiers::NONE, &ask),
            Action::Run {
                command: "ls".into(),
                cd: None
            }
        );
    }

    #[test]
    fn key_bar() {
        let b = blocks();
        let v = view(&b);
        let mut br = Browse::new(4, 0);
        let (status, keys) = br.hint(&v);
        assert_eq!(status, "✕ exit 2  make");
        assert!(keys.iter().any(|k| k.1 == "more keys"));
        br.key(Key::Questionmark, &Modifiers::NONE, &v);
        assert!(br.hint(&v).1.iter().any(|k| k.1 == "copy output"));
    }

    #[test]
    fn typing_a_command() {
        assert_eq!(type_command("ls -l", None, true, false), b"\x05\x15ls -l\r");
        assert_eq!(
            type_command("ls", None, false, true),
            b"\x1bccls",
            "vi keys, edit only"
        );
        assert_eq!(
            type_command("make", Some("/my dir"), true, false),
            b"\x05\x15cd '/my dir' && make\r"
        );
    }
}
