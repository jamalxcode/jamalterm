//! One tab's shell: a PTY running the shell, the terminal emulator it draws
//! into (alacritty_terminal), and the commands found through the shell
//! integration marks. Output is read on its own thread.

use crate::marks::{Mark, Scanner};
use crate::shell::{self, ShellSpec};
use alacritty_terminal::event::{Event, EventListener};
use alacritty_terminal::grid::Dimensions;
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
}

impl Block {
    pub fn running(&self) -> bool {
        self.start.is_some() && self.exit.is_none()
    }
}

/// What the commands look like so far: built up from the marks.
#[derive(Debug, Default, Clone)]
pub struct Commands {
    pub blocks: Vec<Block>,
    pending: Option<Block>,
}

impl Commands {
    /// A mark seen at absolute line `line`.
    pub fn apply(&mut self, mark: &Mark, line: i64, now: Instant) {
        match mark {
            Mark::Prompt => {
                // An empty Enter or Ctrl+C at the prompt: start over.
                self.pending = Some(Block {
                    prompt: line,
                    start: None,
                    end: None,
                    command: String::new(),
                    exit: None,
                    started_at: None,
                    finished_at: None,
                });
            }
            Mark::Command(c) => {
                let p = self.pending.get_or_insert_with(|| Block {
                    prompt: line,
                    start: None,
                    end: None,
                    command: String::new(),
                    exit: None,
                    started_at: None,
                    finished_at: None,
                });
                p.command = c.clone();
            }
            Mark::Start => {
                // fish 4 marks the start itself too: only the first counts.
                if let Some(mut b) = self.pending.take() {
                    b.start = Some(line);
                    b.started_at = Some(now);
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
            Mark::Cwd(_) => {}
        }
    }

    // Known limit: lines are counted as "history + screen line", which stays
    // right until the scrollback is full; after that, old lines drop off the
    // top without alacritty_terminal saying how many, and older marks drift.
    // (docs/DESIGN.md, "Rust port".)

    pub fn running(&self) -> Option<&Block> {
        self.blocks.last().filter(|b| b.running())
    }

    pub fn last_finished(&self) -> Option<&Block> {
        self.blocks.iter().rev().find(|b| b.exit.is_some())
    }
}

/// Everything the window reads to draw a tab.
pub struct Screen {
    pub term: Term<Listener>,
    pub commands: Commands,
    pub cwd: String,
    /// The shell has shown a prompt with jamalterm's hooks.
    pub prompt_seen: bool,
    pub exited: Option<i32>,
}

impl Screen {
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
        let term = Term::new(
            config,
            &TermSize::new(cols as usize, rows as usize),
            listener,
        );
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
                        parser.advance(&mut s.term, &chunk[at..f.end]);
                        at = f.end;
                        let line = s.abs_cursor_line();
                        match &f.mark {
                            Mark::Cwd(dir) => s.cwd = dir.clone(),
                            Mark::Prompt => s.prompt_seen = true,
                            _ => {}
                        }
                        s.commands.apply(&f.mark, line, Instant::now());
                    }
                    parser.advance(&mut s.term, &chunk[at..]);
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
mod tests {
    use super::*;
    use std::time::Duration;

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
            let s = Session::spawn(&spec, 100, 30, 1000, Arc::new(|| {})).unwrap();
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
            let s = Session::spawn(&spec, 100, 30, 1000, Arc::new(|| {})).unwrap();
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
