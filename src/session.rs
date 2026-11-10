//! One tab's shell: a PTY running the shell, the terminal emulator it draws
//! into (alacritty_terminal), and the commands found through the shell
//! integration marks. Output is read on its own thread.

use crate::marks::{Mark, Scanner};
use crate::shell::{self, ShellSpec};
use alacritty_terminal::event::{Event, EventListener};
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::term::test::TermSize;
use alacritty_terminal::term::{Config, Term};
use alacritty_terminal::vte::ansi::Processor;
use portable_pty::{MasterPty, PtySize, native_pty_system};
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// A command found through the marks. Lines are absolute: lines scrolled into
/// the history plus the line on screen (see `Screen::abs_line`).
#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    pub prompt: i64,
    pub start: Option<i64>,
    pub end: Option<i64>,
    pub command: String,
    pub exit: Option<i32>,
    pub started_at: Option<Instant>,
    pub finished_at: Option<Instant>,
    /// The folder it ran in (re-running it elsewhere asks first).
    pub cwd: String,
}

impl Block {
    fn at(line: i64) -> Block {
        Block {
            prompt: line,
            start: None,
            end: None,
            command: String::new(),
            exit: None,
            started_at: None,
            finished_at: None,
            cwd: String::new(),
        }
    }

    pub fn running(&self) -> bool {
        self.start.is_some() && self.exit.is_none()
    }
}

/// What the commands look like so far: built up from the marks.
#[derive(Debug, Default, Clone)]
pub struct Commands {
    pub blocks: Vec<Block>,
    pending: Option<Block>,
    /// The folder from the last OSC 7.
    cwd: String,
    /// Prompts seen so far (typing since the last one means the prompt isn't empty).
    pub prompts: u64,
}

impl Commands {
    /// A mark seen at absolute line `line`.
    pub fn apply(&mut self, mark: &Mark, line: i64, now: Instant) {
        match mark {
            Mark::Prompt => {
                // An empty Enter or Ctrl+C at the prompt: start over.
                self.pending = Some(Block::at(line));
                self.prompts += 1;
            }
            Mark::Command(c) => {
                let p = self.pending.get_or_insert_with(|| Block::at(line));
                p.command = c.clone();
            }
            Mark::Start => {
                // fish 4 marks the start itself too: only the first counts.
                if let Some(mut b) = self.pending.take() {
                    b.start = Some(line);
                    b.started_at = Some(now);
                    b.cwd = self.cwd.clone();
                    self.blocks.push(b);
                }
            }
            Mark::Done(code) => {
                // The innermost command still running (ssh with browse mode on
                // the server nests commands); a repeated "done" finds none.
                if let Some(b) = self.blocks.iter_mut().rev().find(|b| b.running()) {
                    b.exit = Some(*code);
                    b.end = Some(line);
                    b.finished_at = Some(now);
                }
            }
            Mark::Cwd(dir) => self.cwd = dir.clone(),
        }
    }

    /// Old lines dropped off the top of a full history: every line moves up,
    /// and commands that scrolled out are forgotten.
    pub fn shift_up(&mut self, n: i64) {
        for b in self.blocks.iter_mut().chain(self.pending.iter_mut()) {
            b.prompt -= n;
            b.start = b.start.map(|l| l - n);
            b.end = b.end.map(|l| l - n);
        }
        self.blocks.retain(|b| b.prompt >= 0);
    }

    pub fn running(&self) -> Option<&Block> {
        self.blocks.last().filter(|b| b.running())
    }

    pub fn last_finished(&self) -> Option<&Block> {
        self.blocks.iter().rev().find(|b| b.exit.is_some())
    }
}

/// How many lines dropped off the top: the lines that were at `anchor..` are
/// found again `k` lines higher (smallest k first). None if they're gone too.
pub fn dropped_lines(
    before: &[String],
    anchor: i64,
    line: impl Fn(i64) -> String,
) -> Option<usize> {
    (0..=anchor.max(0))
        .find(|&k| {
            before
                .iter()
                .enumerate()
                .all(|(i, want)| line(anchor - k + i as i64) == *want)
        })
        .map(|k| k as usize)
}

/// Everything the window reads to draw a tab.
pub struct Screen {
    pub term: Term<Listener>,
    pub commands: Commands,
    pub cwd: String,
    /// The shell has shown a prompt with jamalterm's hooks.
    pub prompt_seen: bool,
    pub exited: Option<i32>,
    /// The most lines the history keeps (Settings → Scrollback).
    pub max_history: usize,
}

impl Screen {
    /// The text of a line, counted from the top of the history (0 = oldest).
    pub fn line_text(&self, abs: i64) -> String {
        let g = self.term.grid();
        let line = Line((abs - g.history_size() as i64) as i32);
        if line < Line(-(g.history_size() as i32)) || line.0 >= g.screen_lines() as i32 {
            return String::new();
        }
        let row = &g[line];
        let mut out: String = (0..g.columns()).map(|c| row[Column(c)].c).collect();
        out.truncate(out.trim_end().len());
        out
    }

    /// Feeds output to the emulator. When the history is full, old lines drop
    /// off the top and alacritty_terminal doesn't say how many, so the oldest
    /// lines are noted first and found again afterwards: how far they moved is
    /// how many dropped, and every command's lines move up by that much.
    pub fn feed(&mut self, parser: &mut Processor, bytes: &[u8]) {
        let history = self.term.grid().history_size();
        let full = self.max_history > 0 && history >= self.max_history && history >= 3;
        // Lines just above the screen: they survive and move up by exactly
        // the number of lines that dropped off the top.
        let anchor = history as i64 - 3;
        let before: Vec<String> = if full {
            (0..3).map(|i| self.line_text(anchor + i)).collect()
        } else {
            vec![]
        };
        parser.advance(&mut self.term, bytes);
        if !full {
            return;
        }
        let newlines = bytes.iter().filter(|&&b| b == b'\n').count();
        let dropped = if before.iter().all(|l| l.is_empty()) {
            newlines // nothing to recognise: estimate
        } else {
            dropped_lines(&before, anchor, |i| self.line_text(i)).unwrap_or(newlines)
        };
        if dropped > 0 {
            self.commands.shift_up(dropped as i64);
        }
    }

    /// The absolute line of the cursor (history above + line on screen).
    pub fn abs_cursor_line(&self) -> i64 {
        let g = self.term.grid();
        g.history_size() as i64 + g.cursor.point.line.0 as i64
    }
}

/// Receives the emulator's requests: answers to the shell, title, clipboard.
#[derive(Clone)]
pub struct Listener {
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
    repaint: Arc<dyn Fn() + Send + Sync>,
}

impl EventListener for Listener {
    fn send_event(&self, event: Event) {
        match event {
            // Replies to the shell's questions (cursor position, device attributes…).
            Event::PtyWrite(s) => {
                if let Ok(mut w) = self.writer.lock() {
                    let _ = w.write_all(s.as_bytes());
                    let _ = w.flush();
                }
            }
            // Programs (also over ssh/tmux) may copy to the clipboard (OSC 52).
            Event::ClipboardStore(_, text) => {
                if let Ok(mut c) = arboard::Clipboard::new() {
                    let _ = c.set_text(text);
                }
            }
            Event::Wakeup => (self.repaint)(),
            _ => {}
        }
    }
}

pub struct Session {
    pub screen: Arc<Mutex<Screen>>,
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
    master: Box<dyn MasterPty + Send>,
    child: Arc<Mutex<Box<dyn portable_pty::Child + Send + Sync>>>,
}

impl Session {
    /// Starts the shell. `repaint` is called whenever there's new output.
    pub fn spawn(
        spec: &ShellSpec,
        cols: u16,
        rows: u16,
        scrollback: usize,
        preface: &[u8],
        repaint: Arc<dyn Fn() + Send + Sync>,
    ) -> anyhow::Result<Session> {
        let pty = native_pty_system().openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })?;
        let child = pty.slave.spawn_command(shell::command(spec))?;
        drop(pty.slave);
        let mut reader = pty.master.try_clone_reader()?;
        let writer: Arc<Mutex<Box<dyn Write + Send>>> =
            Arc::new(Mutex::new(pty.master.take_writer()?));
        let listener = Listener {
            writer: writer.clone(),
            repaint: repaint.clone(),
        };
        let config = Config {
            scrolling_history: scrollback,
            ..Config::default()
        };
        let mut term = Term::new(
            config,
            &TermSize::new(cols as usize, rows as usize),
            listener,
        );
        // Restored output goes on screen before the shell starts.
        if !preface.is_empty() {
            let mut p: Processor = Processor::new();
            p.advance(&mut term, preface);
        }
        let cwd = spec
            .dir
            .as_ref()
            .map(|d| d.display().to_string())
            .unwrap_or_default();
        let screen = Arc::new(Mutex::new(Screen {
            term,
            commands: Commands::default(),
            cwd,
            prompt_seen: false,
            exited: None,
            max_history: scrollback,
        }));
        let child = Arc::new(Mutex::new(child));

        let (scr, ch, rp) = (screen.clone(), child.clone(), repaint);
        std::thread::Builder::new()
            .name("pty-reader".into())
            .spawn(move || {
                let mut parser: Processor = Processor::new();
                let mut scanner = Scanner::default();
                let mut buf = vec![0u8; 64 * 1024];
                loop {
                    let n = match reader.read(&mut buf) {
                        Ok(0) | Err(_) => break, // the shell side closed
                        Ok(n) => n,
                    };
                    let chunk = &buf[..n];
                    let found = scanner.scan(chunk);
                    // A panic elsewhere while holding the lock must not stop the output.
                    let mut s = scr.lock().unwrap_or_else(|e| e.into_inner());
                    let mut at = 0;
                    // Feed up to each mark, then note where the cursor is.
                    for f in found {
                        s.feed(&mut parser, &chunk[at..f.end]);
                        at = f.end;
                        let line = s.abs_cursor_line();
                        match &f.mark {
                            Mark::Cwd(dir) => s.cwd = dir.clone(),
                            Mark::Prompt => s.prompt_seen = true,
                            _ => {}
                        }
                        s.commands.apply(&f.mark, line, Instant::now());
                    }
                    s.feed(&mut parser, &chunk[at..]);
                    drop(s);
                    rp();
                }
                let code = ch
                    .lock()
                    .unwrap()
                    .wait()
                    .map(|e| e.exit_code() as i32)
                    .unwrap_or(-1);
                scr.lock().unwrap().exited = Some(code);
                rp();
            })?;
        Ok(Session {
            screen,
            writer,
            master: pty.master,
            child,
        })
    }

    /// Typing or pasted text, to the shell.
    pub fn write(&self, bytes: &[u8]) {
        if let Ok(mut w) = self.writer.lock() {
            let _ = w.write_all(bytes);
            let _ = w.flush();
        }
    }

    /// The window or font changed size: tell the shell and the emulator.
    pub fn resize(&self, cols: u16, rows: u16) {
        if cols == 0 || rows == 0 {
            return;
        }
        let _ = self.master.resize(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        });
        self.screen
            .lock()
            .unwrap()
            .term
            .resize(TermSize::new(cols as usize, rows as usize));
    }

    /// Ends the shell (closing a tab).
    pub fn kill(&self) {
        let _ = self.child.lock().unwrap().kill();
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.kill();
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::time::Duration;

    /// A restored pane: its saved output is on screen, then the new prompt
    /// and commands below it, with their marks on the right lines.
    #[test]
    fn restored_output_comes_first() {
        let Some(zsh) = ["/bin/zsh", "/usr/bin/zsh"]
            .into_iter()
            .find(|p| std::path::Path::new(p).exists())
        else {
            return eprintln!("zsh not installed, skipped");
        };
        let home = tempfile::tempdir().unwrap();
        let spec = ShellSpec {
            shell: zsh.into(),
            args: Some(vec![]),
            dir: Some(home.path().to_path_buf()),
            integration: true,
            extra_env: vec![("HOME".into(), home.path().display().to_string())],
        };
        let saved = b"\x1b[0;32mold output\x1b[0m\r\nline two\r\n";
        let s = Session::spawn(&spec, 80, 24, 1000, saved, Arc::new(|| {})).unwrap();
        let until = Instant::now() + Duration::from_secs(15);
        let mut done = false;
        while Instant::now() < until && !done {
            if s.screen.lock().unwrap().prompt_seen && !done {
                s.write(b"echo after\r");
                done = true;
            }
            std::thread::sleep(Duration::from_millis(30));
        }
        assert!(done, "no prompt");
        let until = Instant::now() + Duration::from_secs(10);
        loop {
            let sc = s.screen.lock().unwrap();
            if let Some(b) = sc
                .commands
                .blocks
                .iter()
                .find(|b| b.command == "echo after")
            {
                assert_eq!(sc.line_text(0), "old output");
                assert_eq!(sc.line_text(1), "line two");
                assert!(b.prompt >= 2, "the new prompt is below the old output");
                // (macOS's zsh may mark the prompt a line above the command.)
                assert!(
                    (b.prompt..b.prompt + 3).any(|l| sc.line_text(l).contains("echo after")),
                    "the command is at its prompt"
                );
                break;
            }
            drop(sc);
            assert!(Instant::now() < until, "the command never finished");
            std::thread::sleep(Duration::from_millis(30));
        }
    }

    /// Real shells, with jamalterm's hooks, in a PTY: each command is found
    /// with its text and exit code (3.x's TestIntegration, ported).
    #[test]
    fn real_shells() {
        for sh in ["zsh", "bash", "tcsh", "fish"] {
            let Some(path) = [
                "/bin/",
                "/usr/bin/",
                "/opt/homebrew/bin/",
                "/usr/local/bin/",
            ]
            .iter()
            .map(|d| format!("{d}{sh}"))
            .find(|p| std::path::Path::new(p).exists()) else {
                eprintln!("{sh} not installed, skipped");
                continue;
            };
            let home = tempfile::tempdir().unwrap();
            let spec = ShellSpec {
                shell: path.clone(),
                args: Some(vec![]), // not a login shell: independent of the machine's dotfiles
                dir: Some(home.path().to_path_buf()),
                integration: true,
                extra_env: vec![
                    ("HOME".into(), home.path().display().to_string()),
                    (
                        "XDG_CONFIG_HOME".into(),
                        home.path().join(".config").display().to_string(),
                    ),
                ],
            };
            let s = Session::spawn(&spec, 100, 30, 1000, b"", Arc::new(|| {})).unwrap();
            let wait = |what: &str, f: &dyn Fn(&Screen) -> bool| {
                let until = Instant::now() + Duration::from_secs(15);
                while Instant::now() < until {
                    if f(&s.screen.lock().unwrap()) {
                        return;
                    }
                    std::thread::sleep(Duration::from_millis(30));
                }
                panic!("{sh}: timed out waiting for {what}");
            };
            wait("the first prompt", &|sc| sc.prompt_seen);
            s.write(b"echo hello\r");
            wait("echo", &|sc| {
                sc.commands
                    .blocks
                    .iter()
                    .any(|b| b.command == "echo hello" && b.exit == Some(0))
            });
            s.write(b"sh -c 'exit 7'\r");
            wait("exit 7", &|sc| {
                sc.commands.blocks.iter().any(|b| b.exit == Some(7))
            });
            s.write(b"cd /tmp\r");
            wait("cd", &|sc| sc.cwd.ends_with("/tmp"));
            let sc = s.screen.lock().unwrap();
            let cmds: Vec<&str> = sc
                .commands
                .blocks
                .iter()
                .map(|b| b.command.as_str())
                .collect();
            assert_eq!(
                &cmds[..2],
                &["echo hello", "sh -c 'exit 7'"],
                "{sh}: commands as typed"
            );
            assert!(
                sc.commands.blocks[0].prompt < sc.commands.blocks[1].prompt,
                "{sh}: lines go down"
            );
        }
    }

    /// A screen without a shell, for feeding output by hand.
    pub(crate) fn screen(cols: usize, rows: usize, history: usize) -> Screen {
        let listener = Listener {
            writer: Arc::new(Mutex::new(Box::new(std::io::sink()))),
            repaint: Arc::new(|| {}),
        };
        let term = Term::new(
            Config {
                scrolling_history: history,
                ..Config::default()
            },
            &TermSize::new(cols, rows),
            listener,
        );
        Screen {
            term,
            commands: Commands::default(),
            cwd: String::new(),
            prompt_seen: false,
            exited: None,
            max_history: history,
        }
    }

    /// Feeds output the way the reader thread does: up to each mark, then the
    /// mark at the cursor line.
    pub(crate) fn feed_all(s: &mut Screen, bytes: &[u8]) {
        let mut parser = Processor::new();
        let mut scanner = Scanner::default();
        let mut at = 0;
        for f in scanner.scan(bytes) {
            s.feed(&mut parser, &bytes[at..f.end]);
            at = f.end;
            let line = s.abs_cursor_line();
            s.commands.apply(&f.mark, line, Instant::now());
        }
        s.feed(&mut parser, &bytes[at..]);
    }

    fn command(cmd: &str, out_lines: usize) -> Vec<u8> {
        let b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, cmd);
        let mut v =
            format!("\x1b]133;A\x07$ {cmd}\r\n\x1b]6973;cmd;{b64}\x07\x1b]133;C\x07").into_bytes();
        for i in 0..out_lines {
            v.extend(format!("{cmd} output {i}\r\n").as_bytes());
        }
        v.extend(b"\x1b]133;D;0\x07");
        v
    }

    /// Marks stay on their own lines after the history fills up and old
    /// lines drop off the top.
    #[test]
    fn marks_stay_put_when_the_history_is_full() {
        let mut s = screen(40, 5, 30);
        feed_all(&mut s, &command("first", 3));
        feed_all(&mut s, &command("second", 4));
        for b in &s.commands.blocks {
            assert_eq!(
                s.line_text(b.prompt),
                format!("$ {}", b.command),
                "before the history is full"
            );
        }
        // Lots of output: the history fills and keeps dropping lines.
        for n in 0..6 {
            feed_all(&mut s, &command(&format!("cmd{n}"), 9));
        }
        assert!(s.term.grid().history_size() >= 30);
        assert!(!s.commands.blocks.is_empty());
        for b in &s.commands.blocks {
            assert_eq!(
                s.line_text(b.prompt),
                format!("$ {}", b.command),
                "{} after lines dropped",
                b.command
            );
        }
        assert!(
            !s.commands.blocks.iter().any(|b| b.command == "first"),
            "scrolled out: forgotten"
        );
    }

    #[test]
    fn dropped_line_count() {
        let lines: Vec<String> = (0..10).map(|i| format!("l{i}")).collect();
        let before = vec!["l5".to_string(), "l6".to_string(), "l7".to_string()];
        // l5..l7 were at 7..9 before: 2 lines dropped.
        assert_eq!(
            dropped_lines(&before, 7, |i| lines
                .get(i as usize)
                .cloned()
                .unwrap_or_default()),
            Some(2)
        );
        assert_eq!(
            dropped_lines(&["gone".into()], 7, |i| lines
                .get(i as usize)
                .cloned()
                .unwrap_or_default()),
            None
        );
    }

    #[test]
    fn commands_from_marks() {
        let mut c = Commands::default();
        let t = Instant::now();
        c.apply(&Mark::Prompt, 0, t);
        c.apply(&Mark::Command("ls".into()), 0, t);
        c.apply(&Mark::Start, 1, t);
        c.apply(&Mark::Done(0), 3, t);
        c.apply(&Mark::Done(0), 3, t); // fish 4 repeats it: ignored
        c.apply(&Mark::Prompt, 3, t);
        c.apply(&Mark::Prompt, 4, t); // empty Enter: replaces the pending one
        c.apply(&Mark::Command("false".into()), 4, t);
        c.apply(&Mark::Start, 5, t);
        c.apply(&Mark::Start, 5, t); // fish 4 again: only the first counts
        assert_eq!(c.blocks.len(), 2);
        assert_eq!(
            (
                c.blocks[0].command.as_str(),
                c.blocks[0].exit,
                c.blocks[0].end
            ),
            ("ls", Some(0), Some(3))
        );
        assert_eq!(c.blocks[1].prompt, 4);
        assert!(c.running().is_some());
        c.apply(&Mark::Done(1), 6, t);
        assert_eq!(c.last_finished().unwrap().exit, Some(1));
    }
}

/// The shell setups people use (oh-my-zsh, Powerlevel10k with instant prompt,
/// prezto, starship in zsh, bash and fish), with jamalterm's hooks: CI's
/// "shells" job clones them into $JT_SHELL_SETUPS. Skipped without it.
#[cfg(test)]
mod setups {
    use super::*;
    use std::path::Path;
    use std::time::Duration;

    fn which(name: &str) -> Option<String> {
        std::env::var("PATH")
            .ok()?
            .split(':')
            .map(|d| format!("{d}/{name}"))
            .find(|p| Path::new(p).exists())
    }

    fn check(shell: &str, rc_file: &str, rc: &str, home: &Path, starts: usize) {
        let path = which(shell).unwrap_or_else(|| panic!("{shell} not installed"));
        let full = home.join(rc_file);
        std::fs::create_dir_all(full.parent().unwrap()).unwrap();
        std::fs::write(&full, rc).unwrap();
        for _ in 0..starts {
            let spec = ShellSpec {
                shell: path.clone(),
                args: None, // a login shell, as in the app
                dir: Some(home.to_path_buf()),
                integration: true,
                extra_env: vec![
                    ("HOME".into(), home.display().to_string()),
                    (
                        "XDG_CONFIG_HOME".into(),
                        home.join(".config").display().to_string(),
                    ),
                    (
                        "XDG_CACHE_HOME".into(),
                        home.join(".cache").display().to_string(),
                    ),
                    (
                        "STARSHIP_CONFIG".into(),
                        home.join("starship.toml").display().to_string(),
                    ),
                ],
            };
            let s = Session::spawn(&spec, 100, 30, 1000, b"", Arc::new(|| {})).unwrap();
            let wait = |what: &str, f: &dyn Fn(&Screen) -> bool| {
                let until = Instant::now() + Duration::from_secs(40);
                while Instant::now() < until {
                    if f(&s.screen.lock().unwrap()) {
                        return;
                    }
                    std::thread::sleep(Duration::from_millis(50));
                }
                panic!("{rc_file}: timed out waiting for {what}");
            };
            wait("the first prompt", &|sc| sc.prompt_seen);
            s.write(b"echo setup-ok\r");
            wait("echo", &|sc| {
                sc.commands
                    .blocks
                    .iter()
                    .any(|b| b.command == "echo setup-ok" && b.exit == Some(0))
            });
            s.write(b"sh -c 'exit 7'\r");
            wait("exit 7", &|sc| {
                sc.commands
                    .blocks
                    .iter()
                    .any(|b| b.command == "sh -c 'exit 7'" && b.exit == Some(7))
            });
            let sc = s.screen.lock().unwrap();
            let typed: Vec<&str> = sc
                .commands
                .blocks
                .iter()
                .map(|b| b.command.as_str())
                .collect();
            assert_eq!(
                typed,
                vec!["echo setup-ok", "sh -c 'exit 7'"],
                "{rc_file}: nothing else counted as a command"
            );
        }
    }

    #[test]
    fn real_shell_setups() {
        let Ok(fw) = std::env::var("JT_SHELL_SETUPS") else {
            eprintln!("JT_SHELL_SETUPS not set, skipped");
            return;
        };
        let new_home = || tempfile::tempdir().unwrap();
        let omz = format!(
            "export ZSH=\"{fw}/ohmyzsh\"\nZSH_THEME=robbyrussell\nDISABLE_AUTO_UPDATE=true\nplugins=(git)\nsource $ZSH/oh-my-zsh.sh\n"
        );
        let h = new_home();
        check("zsh", ".zshrc", &omz, h.path(), 1);

        let instant = "if [[ -r \"${XDG_CACHE_HOME:-$HOME/.cache}/p10k-instant-prompt-${(%):-%n}.zsh\" ]]; then\n  source \"${XDG_CACHE_HOME:-$HOME/.cache}/p10k-instant-prompt-${(%):-%n}.zsh\"\nfi\n";
        let h = new_home();
        std::fs::copy(
            format!("{fw}/powerlevel10k/config/p10k-lean.zsh"),
            h.path().join(".p10k.zsh"),
        )
        .unwrap();
        let p10k = format!(
            "{instant}POWERLEVEL9K_DISABLE_CONFIGURATION_WIZARD=true\nsource \"{fw}/powerlevel10k/powerlevel10k.zsh-theme\"\nsource ~/.p10k.zsh\n"
        );
        check("zsh", ".zshrc", &p10k, h.path(), 2); // instant prompt only kicks in from the second start

        let h = new_home();
        std::os::unix::fs::symlink(format!("{fw}/prezto"), h.path().join(".zprezto")).unwrap();
        std::fs::copy(
            format!("{fw}/prezto/runcoms/zpreztorc"),
            h.path().join(".zpreztorc"),
        )
        .unwrap();
        check(
            "zsh",
            ".zshrc",
            "source \"${ZDOTDIR:-$HOME}/.zprezto/init.zsh\"\n",
            h.path(),
            1,
        );

        if which("starship").is_some() {
            check(
                "zsh",
                ".zshrc",
                "eval \"$(starship init zsh)\"\n",
                new_home().path(),
                1,
            );
            check(
                "bash",
                ".bash_profile",
                "eval \"$(starship init bash)\"\n",
                new_home().path(),
                1,
            );
            if which("fish").is_some() {
                check(
                    "fish",
                    ".config/fish/config.fish",
                    "starship init fish | source\n",
                    new_home().path(),
                    1,
                );
            }
        }
    }
}
