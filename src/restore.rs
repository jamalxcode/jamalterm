//! Session restore: every window comes back after a restart with its tabs,
//! splits, folders, recent output and its size and position.
//!
//! The file is 3.x's `session.json` (0600), unchanged, so a 3.x session
//! restores in 4.x and back: `{"version":3,"windows":[…]}`, each window
//! `{"active":n,"groups":[node…],"frame":{x,y,w,h}}`, a node either
//! `{"tab":{"cwd","output"}}` or `{"dir":"row"|"column","ratio","a","b"}`.
//! `output` is terminal text with colors (xterm's serialize in 3.x; ours
//! below), fed to the new emulator before the shell starts.

use crate::panes::{Dir, Node};
use alacritty_terminal::Term;
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::term::cell::{Cell, Flags};
use alacritty_terminal::vte::ansi::{Color, NamedColor};
use serde::{Deserialize, Serialize};
use std::fmt::Write as _;
use std::path::Path;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct SavedTab {
    /// 3.x's shell number (for reattaching a reloaded page); kept, unused.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<u64>,
    #[serde(default)]
    pub cwd: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub share_output: Option<bool>,
    #[serde(default)]
    pub output: String,
    /// The pane with the focus in its tab (new in 4.x; 3.x ignores it).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub focused: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(untagged)]
pub enum SavedNode {
    Leaf {
        tab: SavedTab,
    },
    Split {
        dir: String, // "row": side by side; "column": one above the other
        ratio: f32,
        a: Box<SavedNode>,
        b: Box<SavedNode>,
    },
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Frame {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct SavedWindow {
    #[serde(default)]
    pub active: usize,
    #[serde(default)]
    pub groups: Vec<SavedNode>,
    /// Version 1: one terminal per tab.
    #[serde(default, skip_serializing)]
    pub tabs: Vec<SavedTab>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frame: Option<Frame>,
}

impl SavedWindow {
    /// The tabs, whichever version wrote them.
    pub fn groups(&self) -> Vec<SavedNode> {
        if self.groups.is_empty() {
            self.tabs
                .iter()
                .map(|t| SavedNode::Leaf { tab: t.clone() })
                .collect()
        } else {
            self.groups.clone()
        }
    }
}

#[derive(Serialize, Deserialize)]
struct File {
    version: u32,
    windows: Vec<serde_json::Value>,
}

/// Every saved window, oldest first. A missing or damaged file: none.
/// Files from 2.x held a single window directly.
pub fn decode(data: &str) -> Vec<SavedWindow> {
    if let Ok(f) = serde_json::from_str::<File>(data)
        && f.version >= 3
    {
        return f
            .windows
            .into_iter()
            .filter_map(|w| serde_json::from_value(w).ok())
            .collect();
    }
    serde_json::from_str::<SavedWindow>(data)
        .ok()
        .filter(|w| !w.groups().is_empty())
        .into_iter()
        .collect()
}

pub fn encode(windows: &[SavedWindow]) -> String {
    let f = File {
        version: 3,
        windows: windows
            .iter()
            .filter_map(|w| serde_json::to_value(w).ok())
            .collect(),
    };
    serde_json::to_string(&f).unwrap_or_default()
}

pub fn load(path: &Path) -> Vec<SavedWindow> {
    std::fs::read_to_string(path)
        .map(|d| decode(&d))
        .unwrap_or_default()
}

/// Writes the file atomically, readable only by you (it holds output).
pub fn save(path: &Path, data: &str) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, data)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600))?;
    }
    std::fs::rename(tmp, path)
}

pub fn dir_name(d: Dir) -> &'static str {
    match d {
        Dir::Right => "row",
        Dir::Down => "column",
    }
}

pub fn dir_of(name: &str) -> Dir {
    if name == "column" {
        Dir::Down
    } else {
        Dir::Right
    }
}

/// A saved tree as a tree of saved panes (to be started one by one).
pub fn to_node(n: &SavedNode) -> Node<SavedTab> {
    match n {
        SavedNode::Leaf { tab } => Node::Leaf(tab.clone()),
        SavedNode::Split { dir, ratio, a, b } => Node::Split {
            dir: dir_of(dir),
            ratio: ratio.clamp(0.1, 0.9),
            a: Box::new(to_node(a)),
            b: Box::new(to_node(b)),
        },
    }
}

/// How many lines of output are kept: 1000, or 200 when that's over 400 KB
/// (as 3.x).
pub const LINES: usize = 1000;
const MAX_BYTES: usize = 400_000;

/// The output above the shell's cursor line as terminal text with colors.
/// The live prompt line is left out: the restored shell draws a new one.
pub fn serialize<T>(term: &Term<T>) -> String {
    let out = serialize_lines(term, LINES);
    if out.len() > MAX_BYTES {
        serialize_lines(term, 200)
    } else {
        out
    }
}

fn serialize_lines<T>(term: &Term<T>, max: usize) -> String {
    let grid = term.grid();
    let cursor = grid.cursor.point.line.0;
    let first = (-(grid.history_size() as i32)).max(cursor - max as i32);
    let cols = term.columns();
    let mut out = String::new();
    let mut lines: Vec<String> = vec![];
    let mut pen = Pen::default();
    for l in first..cursor {
        let row = &grid[Line(l)];
        let wrapped = row[Column(cols - 1)].flags.contains(Flags::WRAPLINE);
        // Trailing blanks go (unless the line runs on into the next).
        let mut end = cols;
        if !wrapped {
            while end > 0 && blank(&row[Column(end - 1)]) {
                end -= 1;
            }
        }
        let mut line = String::new();
        for c in 0..end {
            let cell = &row[Column(c)];
            if cell
                .flags
                .intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER)
            {
                continue;
            }
            pen.change_to(cell, &mut line);
            line.push(cell.c);
            if let Some(z) = cell.zerowidth() {
                line.extend(z);
            }
        }
        if wrapped {
            line.push('\u{0}'); // joined below: no line break
        }
        lines.push(line);
    }
    // No blank lines at the end.
    while lines.last().is_some_and(|l| l.is_empty()) {
        lines.pop();
    }
    for (i, l) in lines.iter().enumerate() {
        if let Some(l) = l.strip_suffix('\u{0}') {
            out.push_str(l);
        } else {
            out.push_str(l);
            if i + 1 < lines.len() {
                out.push_str("\r\n");
            }
        }
    }
    if pen != Pen::default() {
        out.push_str("\x1b[0m");
    }
    out
}

fn blank(c: &Cell) -> bool {
    c.c == ' ' && c.bg == Color::Named(NamedColor::Background) && !c.flags.contains(Flags::INVERSE)
}

/// The colors and styles in effect, so only changes are written.
#[derive(Clone, PartialEq)]
struct Pen {
    fg: Color,
    bg: Color,
    flags: Flags,
}

const STYLE: Flags = Flags::BOLD
    .union(Flags::DIM)
    .union(Flags::ITALIC)
    .union(Flags::UNDERLINE)
    .union(Flags::INVERSE)
    .union(Flags::STRIKEOUT);

impl Default for Pen {
    fn default() -> Self {
        Pen {
            fg: Color::Named(NamedColor::Foreground),
            bg: Color::Named(NamedColor::Background),
            flags: Flags::empty(),
        }
    }
}

impl Pen {
    fn change_to(&mut self, cell: &Cell, out: &mut String) {
        let next = Pen {
            fg: cell.fg,
            bg: cell.bg,
            flags: cell.flags & STYLE,
        };
        if next == *self {
            return;
        }
        let mut sgr = vec!["0".to_string()];
        for (f, code) in [
            (Flags::BOLD, "1"),
            (Flags::DIM, "2"),
            (Flags::ITALIC, "3"),
            (Flags::UNDERLINE, "4"),
            (Flags::INVERSE, "7"),
            (Flags::STRIKEOUT, "9"),
        ] {
            if next.flags.contains(f) {
                sgr.push(code.into());
            }
        }
        if let Some(c) = color(next.fg, 30) {
            sgr.push(c);
        }
        if let Some(c) = color(next.bg, 40) {
            sgr.push(c);
        }
        let _ = write!(out, "\x1b[{}m", sgr.join(";"));
        *self = next;
    }
}

/// An SGR color (base 30 for the text, 40 for the background); None for
/// the default.
fn color(c: Color, base: u8) -> Option<String> {
    match c {
        Color::Named(n) if (n as usize) < 8 => Some(format!("{}", base + n as u8)),
        Color::Named(n) if (n as usize) < 16 => Some(format!("{}", base + 60 + n as u8 - 8)),
        Color::Named(_) => None,
        Color::Indexed(i) => Some(format!("{};5;{i}", base + 8)),
        Color::Spec(rgb) => Some(format!("{};2;{};{};{}", base + 8, rgb.r, rgb.g, rgb.b)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::tests::{feed_all, screen};

    #[test]
    fn reads_3x_files() {
        // As 3.5.0 wrote it (a window with a split tab and a plain tab).
        let data = r#"{"version":3,"windows":[{"active":1,"groups":[
            {"dir":"row","ratio":0.4,"a":{"tab":{"id":1,"cwd":"/tmp","output":"hi\r\n"}},
             "b":{"tab":{"id":2,"cwd":"/","shareOutput":true,"output":""}}},
            {"tab":{"id":3,"cwd":"/Users/me"}}],"frame":{"x":10,"y":20,"w":800,"h":600}}]}"#;
        let w = decode(data);
        assert_eq!(w.len(), 1);
        assert_eq!(w[0].active, 1);
        assert_eq!(
            w[0].frame,
            Some(Frame {
                x: 10,
                y: 20,
                w: 800,
                h: 600
            })
        );
        let g = w[0].groups();
        assert_eq!(g.len(), 2);
        let n = to_node(&g[0]);
        let cwds: Vec<&str> = n.leaves().iter().map(|t| t.cwd.as_str()).collect();
        assert_eq!(cwds, ["/tmp", "/"]);
        assert!(
            matches!(n, Node::Split { dir: Dir::Right, ratio, .. } if (ratio - 0.4).abs() < 1e-6)
        );
        // And back: what 3.x reads.
        let again = decode(&encode(&w));
        assert_eq!(again[0].groups(), g);
    }

    #[test]
    fn older_and_damaged_files() {
        // Version 1 (one terminal per tab, no version field).
        let w = decode(r#"{"active":0,"tabs":[{"cwd":"/a"},{"cwd":"/b"}]}"#);
        assert_eq!(w.len(), 1);
        assert_eq!(w[0].groups().len(), 2);
        assert!(decode("not json").is_empty());
        assert!(decode("").is_empty());
        assert!(decode(r#"{"version":3,"windows":[]}"#).is_empty());
    }

    #[test]
    fn output_comes_back_with_its_colors() {
        let mut s = screen(20, 5, 100);
        feed_all(
            &mut s,
            b"plain\r\n\x1b[1;31mbold red\x1b[0m ok\r\n\x1b[38;5;208morange\x1b[0m\r\nthis line is long enough to wrap\r\nprompt$ ",
        );
        let text = serialize(&s.term);
        assert!(!text.contains("prompt$"), "the live prompt is left out");
        let mut again = screen(20, 5, 100);
        feed_all(&mut again, text.as_bytes());
        for l in 0..5 {
            let want = s.line_text(l);
            assert_eq!(again.line_text(l), want, "line {l}");
        }
        assert!(text.contains("\x1b[0;1;31mbold red"), "{text:?}");
        assert!(text.contains("38;5;208"));
        assert!(
            text.contains("long enough to wrap"),
            "a wrapped line is one line again: {text:?}"
        );
    }

    #[test]
    fn saved_file_is_private() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("session.json");
        save(&p, &encode(&[SavedWindow::default()])).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&p).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        assert_eq!(load(&p).len(), 1);
    }
}
