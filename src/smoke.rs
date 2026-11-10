//! Self-test (JAMALTERM_SMOKE=<dir>): the real app types into its own shell,
//! presses keys as a person would, checks what it sees, saves screenshots and
//! writes report.json, then quits. CI runs it on a Mac and on Linux (xvfb).

use crate::app::Tab;
use crate::commands::Cmd;
use serde::Serialize;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Serialize, Default)]
pub struct Check {
    pub name: String,
    pub ok: bool,
    pub detail: String,
}

/// Whether a step is done: (passed, detail), or None to keep waiting.
type DoneFn = fn(&Smoke, &[Tab], usize) -> Option<(bool, String)>;

/// One step: run `act` once, then wait until `done` (or the time limit) and
/// record the check it reports.
struct Step {
    act: fn(&mut Smoke, &egui::Context, &[Tab], usize),
    done: DoneFn,
    name: &'static str,
    limit: Duration,
}

pub struct Smoke {
    /// Key presses to deliver before the next frame (as if typed).
    pub keys: Vec<egui::Event>,
    /// The find bar's "3 of 17" (None when closed), from the app each frame.
    pub find_status: Option<String>,
    /// Menu commands to run in the first window (items with no shortcut).
    pub cmds: Vec<crate::commands::Cmd>,
    /// Pointer events, one batch per frame (a drag takes several frames).
    pub script: std::collections::VecDeque<Vec<egui::Event>>,
    /// Where the tabs are in the bar (from the app each frame).
    pub tab_rects: Vec<egui::Rect>,
    /// A pane id a step remembers to check later.
    marker: u64,
    /// The palette and Settings are open; the current theme (from the app).
    pub palette_open: bool,
    pub settings_open: bool,
    pub theme: String,
    /// Asks the app to close Settings (its keys can't be typed from here).
    pub close_settings: bool,
    /// A close question is up (from the app), and the answer to give it.
    pub asking: bool,
    pub answer: Option<bool>,
    /// The stand-in AI provider's address, and the requests it got.
    pub ai_server: String,
    ai_requests: Arc<Mutex<Vec<String>>>,
    /// The AI panel (from the app): open, the last command card's state,
    /// the last answer, still answering.
    pub ai_open: bool,
    pub ai_card: Option<String>,
    pub ai_answer: Option<String>,
    pub ai_busy: bool,
    /// Clicks Run (true) or Don't Run on the AI's command.
    pub ai_decide: Option<bool>,
    /// The pane in front is at a password prompt (from the app).
    pub password_prompt: bool,
    /// The programs in Your Day's stats (from the app).
    pub stats_programs: Vec<String>,
    /// The sticky header's command, when it shows (from the app).
    pub sticky: Option<String>,
    /// Broadcast Typing is on (from the app).
    pub broadcast: bool,
    /// The crash notice is showing (from the app).
    pub crash_noticed: bool,
    /// The diagnostics report, once shown.
    pub diagnostics: Option<String>,
    /// Done: the app should quit.
    pub quit: bool,
    /// How many windows are open (from the app each frame).
    pub windows: usize,
    /// Lets the app write session.json (the self-test checks it at the end).
    pub save_session: bool,
    /// Whether the Mac menu bar was installed (None where there is none).
    pub menu_bar: Option<bool>,
    dir: PathBuf,
    started: Instant,
    step: usize,
    acted: bool,
    at: Instant,
    checks: Vec<Check>,
}

fn cmd_mods() -> egui::Modifiers {
    if cfg!(target_os = "macos") {
        egui::Modifiers {
            mac_cmd: true,
            command: true,
            ..egui::Modifiers::NONE
        }
    } else {
        egui::Modifiers {
            ctrl: true,
            shift: true,
            command: true,
            ..egui::Modifiers::NONE
        }
    }
}

/// ⇧ and the shortcut key (⇧⌘ on a Mac; Ctrl+Shift elsewhere anyway).
fn cmd_shift() -> egui::Modifiers {
    egui::Modifiers {
        shift: true,
        ..cmd_mods()
    }
}

fn blocks(tabs: &[Tab], active: usize) -> Vec<crate::session::Block> {
    tabs[active]
        .pane()
        .session
        .screen
        .lock()
        .unwrap()
        .commands
        .blocks
        .clone()
}

const STEPS: &[Step] = &[
    Step {
        name: "first prompt",
        act: |_, _, _, _| {},
        done: |_, tabs, a| {
            tabs[a]
                .pane()
                .session
                .screen
                .lock()
                .unwrap()
                .prompt_seen
                .then(|| (true, String::new()))
        },
        limit: Duration::from_secs(15),
    },
    Step {
        name: "the menu bar (on a Mac)",
        act: |_, _, _, _| {},
        done: |s, _, _| {
            Some(match s.menu_bar {
                Some(ok) => (ok, if ok { "installed" } else { "missing" }.into()),
                None => (true, "none on this system".into()),
            })
        },
        limit: Duration::from_secs(1),
    },
    Step {
        name: "a command and its ✓",
        act: |_, _, tabs, a| tabs[a].pane().session.write(b"echo smoke-ok\r"),
        done: |_, tabs, a| {
            blocks(tabs, a)
                .iter()
                .any(|b| b.command == "echo smoke-ok" && b.exit == Some(0))
                .then(|| (true, String::new()))
        },
        limit: Duration::from_secs(10),
    },
    Step {
        name: "a failed command and its ✕",
        act: |_, _, tabs, a| tabs[a].pane().session.write(b"false\r"),
        done: |_, tabs, a| {
            blocks(tabs, a)
                .iter()
                .any(|b| b.command == "false" && b.exit == Some(1))
                .then(|| (true, String::new()))
        },
        limit: Duration::from_secs(10),
    },
    Step {
        name: "↑ at an empty prompt starts browse mode",
        act: |s, _, _, _| s.press(egui::Key::ArrowUp, egui::Modifiers::NONE),
        done: |_, tabs, a| {
            let row = tabs[a].pane().browse.as_ref()?.row;
            let line = tabs[a].pane().session.screen.lock().unwrap().line_text(row);
            Some((line.ends_with("false"), format!("cursor on {line:?}")))
        },
        limit: Duration::from_secs(5),
    },
    Step {
        name: "(screenshot: browse mode)",
        act: |s, ctx, _, _| s.shot(ctx, "browse"),
        done: |_, _, _| None, // just waits for the frame to be captured
        limit: Duration::from_millis(800),
    },
    Step {
        name: "⏎ in browse mode runs the command again",
        act: |s, _, _, _| s.press(egui::Key::Enter, egui::Modifiers::NONE),
        done: |_, tabs, a| {
            let runs = blocks(tabs, a)
                .iter()
                .filter(|b| b.command == "false")
                .count();
            (runs >= 2).then(|| (tabs[a].pane().browse.is_none(), format!("ran {runs} times")))
        },
        limit: Duration::from_secs(10),
    },
    Step {
        name: "⌘F finds text in the output",
        act: |s, _, _, _| s.press(egui::Key::F, cmd_mods()),
        done: |s, _, _| {
            s.find_status
                .as_ref()
                .map(|_| (true, "the find bar opened".into()))
        },
        limit: Duration::from_secs(3),
    },
    Step {
        name: "typing in the find bar counts the matches",
        act: |s, _, _, _| s.keys.push(egui::Event::Text("smoke".into())),
        done: |s, _, _| {
            let st = s.find_status.clone().unwrap_or_default();
            // "echo smoke-ok" and its output "smoke-ok": at least 2.
            st.ends_with(" of 2")
                .then(|| (true, st.clone()))
                .or_else(|| (st.contains(" of ")).then_some((false, st)))
        },
        limit: Duration::from_secs(3),
    },
    Step {
        name: "(screenshot: find)",
        act: |s, ctx, _, _| s.shot(ctx, "find"),
        done: |_, _, _| None,
        limit: Duration::from_millis(800),
    },
    Step {
        name: "esc closes find",
        act: |s, _, _, _| s.press(egui::Key::Escape, egui::Modifiers::NONE),
        done: |s, _, _| s.find_status.is_none().then(|| (true, String::new())),
        limit: Duration::from_secs(2),
    },
    Step {
        name: "the Esc that closed find didn't reach the shell",
        act: |s, _, _, _| {
            s.keys.push(egui::Event::Text("echo whole".into()));
            s.press(egui::Key::Enter, egui::Modifiers::NONE);
        },
        done: |_, tabs, a| {
            blocks(tabs, a)
                .iter()
                .any(|b| b.exit.is_some() && b.command.ends_with("cho whole"))
                .then(|| {
                    let b = blocks(tabs, a);
                    let ok = b.iter().any(|b| b.command == "echo whole");
                    (
                        ok,
                        if ok {
                            "typed whole".into()
                        } else {
                            "its first letter was lost".into()
                        },
                    )
                })
        },
        limit: Duration::from_secs(5),
    },
    Step {
        name: "⌘D splits the pane",
        act: |s, _, _, _| s.press(egui::Key::D, cmd_mods()),
        done: |_, tabs, a| {
            let t = &tabs[a];
            let n = t.tree().count();
            let ready = t.pane().session.screen.lock().unwrap().prompt_seen;
            (n == 2 && ready && t.focus == t.panes()[1].id)
                .then(|| (true, "2 panes, the new one focused".into()))
        },
        limit: Duration::from_secs(15),
    },
    Step {
        name: "typing goes to the focused pane",
        act: |s, _, _, _| {
            s.keys.push(egui::Event::Text("echo pane-two".into()));
            s.press(egui::Key::Enter, egui::Modifiers::NONE);
        },
        done: |_, tabs, a| {
            let has = |i: usize| {
                tabs[a].panes()[i]
                    .session
                    .screen
                    .lock()
                    .unwrap()
                    .commands
                    .blocks
                    .iter()
                    .any(|b| b.command == "echo pane-two" && b.exit == Some(0))
            };
            has(1).then(|| (!has(0), "only in the new pane".into()))
        },
        limit: Duration::from_secs(10),
    },
    Step {
        name: "(screenshot: split panes)",
        act: |s, ctx, _, _| s.shot(ctx, "split"),
        done: |_, _, _| None,
        limit: Duration::from_millis(800),
    },
    Step {
        name: "⌘W closes the pane, not the tab",
        act: |s, _, _, _| s.press(egui::Key::W, cmd_mods()),
        done: |_, tabs, a| {
            (tabs.len() == 1 && tabs[a].tree().count() == 1).then(|| (true, "1 tab, 1 pane".into()))
        },
        limit: Duration::from_secs(5),
    },
    Step {
        name: "(a program runs)",
        act: |_, _, tabs, a| tabs[a].pane().session.write(b"sleep 30\r"),
        done: |_, tabs, a| tabs[a].pane().session.busy().then(|| (true, String::new())),
        limit: Duration::from_secs(5),
    },
    Step {
        name: "closing a tab while a program runs asks first",
        act: |s, _, _, _| s.press(egui::Key::W, cmd_mods()),
        done: |s, tabs, _| {
            s.asking
                .then(|| (tabs.len() == 1, "the question is up".into()))
        },
        limit: Duration::from_secs(3),
    },
    Step {
        name: "(screenshot: the question)",
        act: |s, ctx, _, _| s.shot(ctx, "question"),
        done: |_, _, _| None,
        limit: Duration::from_millis(800),
    },
    Step {
        name: "Cancel keeps the tab and its program",
        act: |s, _, _, _| s.answer = Some(false),
        done: |s, tabs, a| {
            (!s.asking).then(|| {
                (
                    tabs.len() == 1 && tabs[a].pane().session.busy(),
                    "still running".into(),
                )
            })
        },
        limit: Duration::from_secs(3),
    },
    Step {
        name: "(⌃C stops it)",
        act: |_, _, tabs, a| tabs[a].pane().session.write(b"\x03"),
        done: |_, tabs, a| (!tabs[a].pane().session.busy()).then(|| (true, String::new())),
        limit: Duration::from_secs(5),
    },
    Step {
        name: "(split, and run a program there)",
        act: |s, _, _, _| s.press(egui::Key::D, cmd_mods()),
        done: |_, tabs, a| {
            let t = &tabs[a];
            let ready =
                t.tree().count() == 2 && t.pane().session.screen.lock().unwrap().prompt_seen;
            if ready {
                t.pane().session.write(b"sleep 30\r");
            }
            ready.then(|| (true, String::new()))
        },
        limit: Duration::from_secs(10),
    },
    Step {
        name: "(it runs)",
        act: |_, _, _, _| {},
        done: |_, tabs, a| tabs[a].pane().session.busy().then(|| (true, String::new())),
        limit: Duration::from_secs(5),
    },
    Step {
        name: "closing that pane asks too",
        act: |s, _, _, _| s.press(egui::Key::W, cmd_mods()),
        done: |s, _, _| s.asking.then(|| (true, String::new())),
        limit: Duration::from_secs(3),
    },
    Step {
        name: "Terminate closes the pane",
        act: |s, _, _, _| s.answer = Some(true),
        done: |s, tabs, a| {
            (!s.asking && tabs[a].tree().count() == 1).then(|| (true, "1 pane".into()))
        },
        limit: Duration::from_secs(5),
    },
    Step {
        name: "a password prompt is spotted (for Secure Keyboard Entry)",
        act: |_, _, tabs, a| tabs[a].pane().session.write(b"read -s secret\r"),
        done: |s, _, _| s.password_prompt.then(|| (true, String::new())),
        limit: Duration::from_secs(5),
    },
    Step {
        name: "(answer it)",
        act: |_, _, tabs, a| tabs[a].pane().session.write(b"hunter2\r"),
        done: |s, _, _| (!s.password_prompt).then(|| (true, String::new())),
        limit: Duration::from_secs(5),
    },
    Step {
        name: "Your Day keeps program names only",
        act: |s, _, _, _| s.cmds.push(Cmd::YourDay),
        done: |s, _, _| {
            let p = &s.stats_programs;
            (p.iter().any(|x| x == "echo") && p.iter().any(|x| x == "false")).then(|| {
                (
                    !p.iter().any(|x| x.contains(' ')),
                    format!("{} runs: {}", p.len(), p.join(", ")),
                )
            })
        },
        limit: Duration::from_secs(3),
    },
    Step {
        name: "Copy Diagnostics shows a scrubbed report",
        act: |s, _, _, _| s.cmds.push(Cmd::Diagnostics),
        done: |s, _, _| {
            s.diagnostics.as_ref().map(|d| {
                let ok = d.starts_with("jamalterm diagnostics")
                    && d.contains("Recent problems")
                    && !d.contains("hunter2");
                (ok, format!("{} lines", d.lines().count()))
            })
        },
        limit: Duration::from_secs(3),
    },
    Step {
        name: "(a long output)",
        act: |_, _, tabs, a| tabs[a].pane().session.write(b"seq 1 300\r"),
        done: |_, tabs, a| {
            blocks(tabs, a)
                .iter()
                .any(|b| b.command == "seq 1 300" && b.exit == Some(0))
                .then(|| (true, String::new()))
        },
        limit: Duration::from_secs(10),
    },
    Step {
        name: "scrolled up, the sticky header names the command",
        act: |_, _, tabs, a| {
            use alacritty_terminal::grid::Scroll;
            tabs[a]
                .pane()
                .session
                .screen
                .lock()
                .unwrap()
                .term
                .scroll_display(Scroll::Delta(120));
        },
        done: |s, _, _| {
            s.sticky
                .as_ref()
                .map(|c| (c == "seq 1 300", format!("header: {c}")))
        },
        limit: Duration::from_secs(3),
    },
    Step {
        name: "(screenshot: sticky header)",
        act: |s, ctx, _, _| s.shot(ctx, "sticky"),
        done: |_, _, _| None,
        limit: Duration::from_millis(800),
    },
    Step {
        name: "(back to the bottom)",
        act: |_, _, tabs, a| {
            use alacritty_terminal::grid::Scroll;
            tabs[a]
                .pane()
                .session
                .screen
                .lock()
                .unwrap()
                .term
                .scroll_display(Scroll::Bottom);
        },
        done: |s, _, _| s.sticky.is_none().then(|| (true, String::new())),
        limit: Duration::from_secs(3),
    },
    Step {
        name: "(split for broadcasting)",
        act: |s, _, _, _| s.press(egui::Key::D, cmd_mods()),
        done: |_, tabs, a| {
            let t = &tabs[a];
            (t.tree().count() == 2 && t.pane().session.screen.lock().unwrap().prompt_seen)
                .then(|| (true, String::new()))
        },
        limit: Duration::from_secs(10),
    },
    Step {
        name: "Broadcast Typing sends keys to every pane",
        act: |s, _, _, _| {
            s.cmds.push(Cmd::Broadcast);
            s.script.push_back(vec![]);
            s.script
                .push_back(vec![egui::Event::Text("echo to-all".into())]);
            s.script.push_back(vec![egui::Event::Key {
                key: egui::Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }]);
        },
        done: |s, tabs, a| {
            let ran = |i: usize| {
                tabs[a].panes()[i]
                    .session
                    .screen
                    .lock()
                    .unwrap()
                    .commands
                    .blocks
                    .iter()
                    .any(|b| b.command == "echo to-all" && b.exit == Some(0))
            };
            (ran(0) && ran(1)).then(|| (s.broadcast, "both panes ran it".into()))
        },
        limit: Duration::from_secs(10),
    },
    Step {
        name: "(stop broadcasting, close the pane)",
        act: |s, _, _, _| {
            s.cmds.push(Cmd::Broadcast);
            s.script.push_back(vec![]);
            s.script.push_back(vec![]);
        },
        done: |s, _, _| (!s.broadcast).then(|| (true, String::new())),
        limit: Duration::from_secs(3),
    },
    Step {
        name: "(close the second pane)",
        act: |s, _, _, _| s.press(egui::Key::W, cmd_mods()),
        done: |_, tabs, a| (tabs[a].tree().count() == 1).then(|| (true, String::new())),
        limit: Duration::from_secs(5),
    },
    Step {
        name: "an inline image (imgcat's protocol) is shown",
        act: |_, _, tabs, a| {
            // A 120×60 gradient as PNG, printed by the shell.
            let (w, h) = (120u32, 60u32);
            let mut png_bytes = vec![];
            {
                let mut e = png::Encoder::new(&mut png_bytes, w, h);
                e.set_color(png::ColorType::Rgb);
                e.set_depth(png::BitDepth::Eight);
                let mut wr = e.write_header().unwrap();
                let px: Vec<u8> = (0..h)
                    .flat_map(|y| (0..w).flat_map(move |x| [(x * 2) as u8, (y * 4) as u8, 200]))
                    .collect();
                wr.write_image_data(&px).unwrap();
            }
            use base64::Engine;
            let b64 = base64::engine::general_purpose::STANDARD.encode(png_bytes);
            let cmd = format!("printf '\\033]1337;File=inline=1:{b64}\\a\\n'\r");
            tabs[a].pane().session.write(cmd.as_bytes());
        },
        done: |_, tabs, a| {
            let s = tabs[a].pane().session.screen.lock().unwrap();
            s.images
                .first()
                .map(|im| (im.rows >= 2, format!("{} lines tall", im.rows)))
        },
        limit: Duration::from_secs(5),
    },
    Step {
        name: "(screenshot: image)",
        act: |s, ctx, _, _| s.shot(ctx, "image"),
        done: |_, _, _| None,
        limit: Duration::from_millis(800),
    },
    Step {
        name: "⇧⌘P opens the command palette",
        act: |s, _, _, _| s.press(egui::Key::P, cmd_shift()),
        done: |s, _, _| s.palette_open.then(|| (true, String::new())),
        limit: Duration::from_secs(3),
    },
    Step {
        name: "(type in the palette)",
        act: |s, _, _, _| s.keys.push(egui::Event::Text("split right".into())),
        done: |_, _, _| None,
        limit: Duration::from_millis(300),
    },
    Step {
        name: "(screenshot: palette)",
        act: |s, ctx, _, _| s.shot(ctx, "palette"),
        done: |_, _, _| None,
        limit: Duration::from_millis(800),
    },
    Step {
        name: "⏎ in the palette runs the command",
        act: |s, _, _, _| s.press(egui::Key::Enter, egui::Modifiers::NONE),
        done: |s, tabs, a| {
            (tabs[a].tree().count() == 2).then(|| {
                (
                    !s.palette_open,
                    "Split Pane Right: 2 panes, palette closed".into(),
                )
            })
        },
        limit: Duration::from_secs(10),
    },
    Step {
        name: "(⌘W closes that pane)",
        act: |s, _, _, _| s.press(egui::Key::W, cmd_mods()),
        done: |_, tabs, a| (tabs[a].tree().count() == 1).then(|| (true, String::new())),
        limit: Duration::from_secs(3),
    },
    Step {
        name: "a theme from the palette applies and is saved",
        act: |s, _, _, _| {
            s.press(egui::Key::P, cmd_shift());
            s.script.push_back(vec![]);
            s.script
                .push_back(vec![egui::Event::Text("theme solarized light".into())]);
            s.script.push_back(vec![]);
            s.script.push_back(vec![
                egui::Event::Key {
                    key: egui::Key::Enter,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                },
                egui::Event::Key {
                    key: egui::Key::Enter,
                    physical_key: None,
                    pressed: false,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                },
            ]);
        },
        done: |s, _, _| {
            let saved = crate::settings::load(&s.dir.join("settings.json"));
            (s.theme == "solarized-light" || saved.theme == "solarized-light")
                .then(|| (saved.theme == s.theme, format!("theme {}", s.theme)))
        },
        limit: Duration::from_secs(5),
    },
    Step {
        name: "⌘, opens Settings",
        act: |s, _, _, _| s.press(egui::Key::Comma, cmd_mods()),
        done: |s, _, _| s.settings_open.then(|| (true, "its own window".into())),
        limit: Duration::from_secs(3),
    },
    Step {
        name: "(Settings stays open a moment)",
        act: |_, _, _, _| {},
        done: |_, _, _| None,
        limit: Duration::from_millis(600),
    },
    Step {
        name: "Settings closes",
        act: |s, _, _, _| s.close_settings = true,
        done: |s, _, _| (!s.settings_open).then(|| (true, String::new())),
        limit: Duration::from_secs(3),
    },
    Step {
        name: "⌘I opens the AI panel",
        act: |s, _, _, _| s.press(egui::Key::I, cmd_mods()),
        done: |s, _, _| s.ai_open.then(|| (true, String::new())),
        limit: Duration::from_secs(3),
    },
    Step {
        name: "a question goes to the AI",
        act: |s, _, _, _| {
            s.keys
                .push(egui::Event::Text("run echo from-ai for me".into()));
            s.script.push_back(vec![]);
            s.script.push_back(vec![egui::Event::Key {
                key: egui::Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }]);
        },
        done: |s, _, _| {
            s.ai_card
                .is_some()
                .then(|| (true, String::new()))
                .or_else(|| {
                    s.ai_answer
                        .as_ref()
                        .filter(|a| a.starts_with("error"))
                        .map(|a| (false, a.clone()))
                })
        },
        limit: Duration::from_secs(10),
    },
    Step {
        name: "the AI's command waits for Run",
        act: |_, _, _, _| {},
        done: |s, tabs, a| {
            let waiting = s.ai_card.as_deref() == Some("Asking");
            let ran = blocks(tabs, a).iter().any(|b| b.command == "echo from-ai");
            Some((waiting && !ran, format!("card: {:?}", s.ai_card)))
        },
        limit: Duration::from_secs(1),
    },
    Step {
        name: "(screenshot: AI panel)",
        act: |s, ctx, _, _| s.shot(ctx, "ai"),
        done: |_, _, _| None,
        limit: Duration::from_millis(800),
    },
    Step {
        name: "Run runs it, and the AI hears how it went",
        act: |s, _, _, _| s.ai_decide = Some(true),
        done: |s, tabs, a| {
            let answered = s.ai_answer.as_deref() == Some("It printed from-ai.") && !s.ai_busy;
            answered.then(|| {
                let ran = blocks(tabs, a)
                    .iter()
                    .any(|b| b.command == "echo from-ai" && b.exit == Some(0));
                let reqs = s.ai_requests.lock().unwrap();
                let told = reqs.get(1).is_some_and(|r| {
                    r.contains("tool_result") && r.contains("exit code 0") && r.contains("from-ai")
                });
                let sent_context = reqs
                    .first()
                    .is_some_and(|r| r.contains("<terminal>") && r.contains("echo smoke-ok"));
                (
                    ran && told && sent_context,
                    format!("ran {ran}, told {told}, context {sent_context}"),
                )
            })
        },
        limit: Duration::from_secs(15),
    },
    Step {
        name: "⌘J: plain English becomes a command at the prompt (not run)",
        act: |s, _, _, _| {
            s.press(egui::Key::J, cmd_mods());
            s.script.push_back(vec![]);
            s.script
                .push_back(vec![egui::Event::Text("say hello".into())]);
            s.script.push_back(vec![egui::Event::Key {
                key: egui::Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }]);
        },
        done: |_, tabs, a| {
            let s = tabs[a].pane().session.screen.lock().unwrap();
            let line = s.line_text(s.abs_cursor_line());
            let ran = s
                .commands
                .blocks
                .iter()
                .any(|b| b.command.contains("from-the-bar"));
            line.contains("echo from-the-bar")
                .then(|| (!ran, format!("prompt: {line:?}")))
        },
        limit: Duration::from_secs(10),
    },
    Step {
        name: "(clear the prompt)",
        act: |_, _, tabs, a| tabs[a].pane().session.write(b"\x05\x15"),
        done: |_, tabs, a| {
            let s = tabs[a].pane().session.screen.lock().unwrap();
            (!s.line_text(s.abs_cursor_line()).contains("from-the-bar"))
                .then(|| (true, String::new()))
        },
        limit: Duration::from_secs(3),
    },
    Step {
        name: "(⌘I hides it)",
        act: |s, _, _, _| s.press(egui::Key::I, cmd_mods()),
        done: |s, _, _| (!s.ai_open).then(|| (true, String::new())),
        limit: Duration::from_secs(3),
    },
    Step {
        name: "⌘T opens a tab",
        act: |s, _, _, _| s.press(egui::Key::T, cmd_mods()),
        done: |_, tabs, _| (tabs.len() == 2).then(|| (true, "2 tabs".into())),
        limit: Duration::from_secs(5),
    },
    Step {
        name: "dragging a tab reorders the bar",
        act: |s, _, tabs, _| s.drag_second_tab_first(tabs),
        done: |s, tabs, a| {
            (s.script.is_empty() && tabs.len() == 2).then(|| {
                (
                    tabs[0].pane().id == s.marker && a == 0,
                    "the second tab is first, and in front".into(),
                )
            })
        },
        limit: Duration::from_secs(5),
    },
    Step {
        name: "and back",
        act: |s, _, tabs, _| s.drag_second_tab_first(tabs),
        done: |s, tabs, _| {
            (s.script.is_empty() && tabs.len() == 2).then(|| {
                let first = tabs[0].pane().session.screen.lock().unwrap();
                let back =
                    (0..first.abs_cursor_line()).any(|l| first.line_text(l).contains("smoke-ok"));
                (back, "the first tab is first again".into())
            })
        },
        limit: Duration::from_secs(5),
    },
    Step {
        name: "⌘2 shows the second tab",
        act: |s, _, _, _| s.press(egui::Key::Num2, cmd_mods()),
        done: |_, _, a| (a == 1).then(|| (true, String::new())),
        limit: Duration::from_secs(3),
    },
    Step {
        name: "Move Tab to New Window",
        act: |s, _, _, _| s.cmds.push(Cmd::MoveTabToNewWindow),
        done: |s, tabs, _| {
            (s.windows == 2 && tabs.len() == 1).then(|| (true, "2 windows, 1 tab here".into()))
        },
        limit: Duration::from_secs(5),
    },
    Step {
        name: "⌘N opens a window",
        act: |s, _, _, _| s.press(egui::Key::N, cmd_mods()),
        done: |s, _, _| (s.windows == 3).then(|| (true, "3 windows".into())),
        limit: Duration::from_secs(5),
    },
    Step {
        name: "Merge All Windows",
        act: |s, _, _, _| s.cmds.push(Cmd::MergeAllWindows),
        done: |s, tabs, _| {
            (s.windows == 1 && tabs.len() == 3).then(|| (true, "1 window, 3 tabs".into()))
        },
        limit: Duration::from_secs(5),
    },
    Step {
        name: "⌘3 and ⌘W close the third tab",
        act: |s, _, _, _| {
            s.press(egui::Key::Num3, cmd_mods());
            s.press(egui::Key::W, cmd_mods());
        },
        done: |s, tabs, _| (s.windows == 1 && tabs.len() == 2).then(|| (true, "2 tabs".into())),
        limit: Duration::from_secs(5),
    },
    Step {
        name: "a second window for the session",
        act: |s, _, _, _| s.press(egui::Key::N, cmd_mods()),
        done: |s, _, _| (s.windows == 2).then(|| (true, "2 windows".into())),
        limit: Duration::from_secs(5),
    },
    Step {
        name: "the session is saved (tabs, folders, output)",
        act: |s, _, _, _| s.save_session = true,
        done: |s, _, _| {
            let w = crate::restore::load(&s.dir.join("session.json"));
            let groups = w.first()?.groups();
            (groups.len() == 2 && w.len() == 2).then(|| {
                let crate::restore::SavedNode::Leaf { tab } = &groups[0] else {
                    return (false, "the first tab was saved split".into());
                };
                (
                    tab.output.contains("smoke-ok") && !tab.cwd.is_empty(),
                    format!("2 windows, 2 tabs, {} bytes of output", tab.output.len()),
                )
            })
        },
        limit: Duration::from_secs(5),
    },
    Step {
        name: "(screenshot: two tabs)",
        act: |s, ctx, _, _| s.shot(ctx, "tabs"),
        done: |_, _, _| None,
        limit: Duration::from_secs(2),
    },
];

/// The second run (JAMALTERM_SMOKE_RESTORE=1): the window the first run
/// saved comes back.
const RESTORE_STEPS: &[Step] = &[
    Step {
        name: "a crash last time is noticed (and not sent anywhere)",
        act: |_, _, _, _| {},
        done: |s, _, _| {
            let want = std::env::var_os("JAMALTERM_SMOKE_EXPECT_CRASH").is_some();
            Some((
                s.crash_noticed == want,
                format!("noticed: {}", s.crash_noticed),
            ))
        },
        limit: Duration::from_secs(1),
    },
    Step {
        name: "the saved tabs come back with their output",
        act: |_, _, _, _| {},
        done: |sm, tabs, _| {
            let first = tabs.first()?;
            let s = first.pane().session.screen.lock().unwrap();
            let text: Vec<String> = (0..s.abs_cursor_line()).map(|l| s.line_text(l)).collect();
            let back = text.iter().any(|l| l.contains("smoke-ok"));
            let marked = text
                .iter()
                .any(|l| l.contains("Restored from the last session"));
            (back && marked && s.prompt_seen).then(|| {
                (
                    tabs.len() == 2 && sm.windows == 2,
                    format!(
                        "{} windows, {} tabs, the output and a new prompt",
                        sm.windows,
                        tabs.len()
                    ),
                )
            })
        },
        limit: Duration::from_secs(15),
    },
    Step {
        name: "(screenshot: restored)",
        act: |s, ctx, _, _| s.shot(ctx, "restored"),
        done: |_, _, _| None,
        limit: Duration::from_millis(800),
    },
];

/// Whether this run checks restoring (see RESTORE_STEPS).
pub fn restore_run() -> bool {
    std::env::var_os("JAMALTERM_SMOKE_RESTORE").is_some()
}

impl Smoke {
    pub fn from_env() -> Option<Smoke> {
        let dir = PathBuf::from(std::env::var_os("JAMALTERM_SMOKE")?);
        let _ = std::fs::create_dir_all(&dir);
        let ai_requests = Arc::new(Mutex::new(vec![]));
        let ai_server = stand_in_ai(ai_requests.clone());
        Some(Smoke {
            keys: vec![],
            find_status: None,
            menu_bar: None,
            save_session: false,
            windows: 1,
            cmds: vec![],
            quit: false,
            password_prompt: false,
            crash_noticed: false,
            sticky: None,
            broadcast: false,
            stats_programs: vec![],
            diagnostics: None,
            ai_server,
            ai_requests,
            ai_open: false,
            ai_card: None,
            ai_answer: None,
            ai_busy: false,
            ai_decide: None,
            asking: false,
            answer: None,
            palette_open: false,
            settings_open: false,
            theme: String::new(),
            close_settings: false,
            script: Default::default(),
            tab_rects: vec![],
            marker: 0,
            dir,
            started: Instant::now(),
            step: 0,
            acted: false,
            at: Instant::now(),
            checks: vec![],
        })
    }

    /// Saves a screenshot of the next frame as <dir>/<name>.png (the
    /// window `ctx` belongs to).
    pub fn shot(&self, ctx: &egui::Context, name: &str) {
        let path = self.dir.join(format!("{name}.png"));
        ctx.request_screenshot(move |img| {
            if let Err(e) = save_png(&path, &img) {
                eprintln!("jamalterm: screenshot {}: {e}", path.display());
            }
        });
    }

    /// Presses a key (down and up).
    fn press(&mut self, key: egui::Key, modifiers: egui::Modifiers) {
        for pressed in [true, false] {
            self.keys.push(egui::Event::Key {
                key,
                physical_key: None,
                pressed,
                repeat: false,
                modifiers,
            });
        }
    }

    /// Drags with the mouse from one point to another, over a few frames.
    fn drag(&mut self, from: egui::Pos2, to: egui::Pos2) {
        let button = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        self.script.push_back(vec![egui::Event::PointerMoved(from)]);
        self.script.push_back(vec![button(from, true)]);
        for k in 1..=6 {
            let p = from + (to - from) * (k as f32 / 6.0);
            self.script.push_back(vec![egui::Event::PointerMoved(p)]);
        }
        self.script.push_back(vec![button(to, false)]);
        self.script.push_back(vec![]);
    }

    /// Drags the second tab in front of the first.
    fn drag_second_tab_first(&mut self, tabs: &[Tab]) {
        if let (Some(a), Some(b)) = (self.tab_rects.first(), self.tab_rects.get(1)) {
            self.marker = tabs[1].pane().id;
            let (from, to) = (b.center(), egui::pos2(a.left() + 4.0, a.center().y));
            self.drag(from, to);
        }
    }

    /// Called every frame.
    pub fn step(&mut self, ctx: &egui::Context, tabs: &[Tab], active: usize) {
        ctx.request_repaint_after(Duration::from_millis(50));
        let steps = if restore_run() { RESTORE_STEPS } else { STEPS };
        let Some(step) = steps.get(self.step) else {
            let report = serde_json::json!({
                "ok": self.checks.iter().all(|c| c.ok),
                "checks": self.checks,
                "firstPromptMs": self.checks.first().map(|_| self.started.elapsed().as_millis()),
                "tabs": tabs.len(),
            });
            let name = if restore_run() {
                "report-restore.json"
            } else {
                "report.json"
            };
            let _ = std::fs::write(
                self.dir.join(name),
                serde_json::to_vec_pretty(&report).unwrap(),
            );
            self.quit = true; // the app quits (every window), saving
            return;
        };
        if !self.acted {
            (step.act)(self, ctx, tabs, active);
            self.acted = true;
            self.at = Instant::now();
            return;
        }
        let result = (step.done)(self, tabs, active);
        let timed_out = self.at.elapsed() > step.limit;
        if result.is_none() && !timed_out {
            return;
        }
        eprintln!(
            "smoke: {} {}",
            step.name,
            match &result {
                Some((true, _)) => "ok",
                Some((false, _)) => "FAILED",
                None if step.name.starts_with('(') => "done",
                None => "timed out",
            }
        );
        if !step.name.starts_with('(') {
            let ms = self.at.elapsed().as_millis();
            let (ok, detail) = result.unwrap_or((false, "timed out".into()));
            self.checks.push(Check {
                name: step.name.into(),
                ok,
                detail: format!("{detail} ({ms} ms)").trim().into(),
            });
        }
        self.step += 1;
        self.acted = false;
    }
}

/// A stand-in for Anthropic's API, so the self-test can check the AI panel
/// end to end without a key or the network: the first answer asks to run
/// `echo from-ai`, the next one (with the result) answers in words.
fn stand_in_ai(requests: Arc<Mutex<Vec<String>>>) -> String {
    use std::io::{BufRead, BufReader, Read, Write};
    let Ok(l) = std::net::TcpListener::bind("127.0.0.1:0") else {
        return String::new();
    };
    let base = format!(
        "http://{}",
        l.local_addr().map(|a| a.to_string()).unwrap_or_default()
    );
    std::thread::spawn(move || {
        for n in 0.. {
            let Ok((mut s, _)) = l.accept() else { return };
            let mut r = BufReader::new(match s.try_clone() {
                Ok(c) => c,
                Err(_) => continue,
            });
            let mut len = 0;
            loop {
                let mut h = String::new();
                if r.read_line(&mut h).unwrap_or(0) == 0 || h == "\r\n" {
                    break;
                }
                if let Some(v) = h.to_ascii_lowercase().strip_prefix("content-length:") {
                    len = v.trim().parse().unwrap_or(0);
                }
            }
            let mut body = vec![0; len];
            let _ = r.read_exact(&mut body);
            requests
                .lock()
                .unwrap()
                .push(String::from_utf8_lossy(&body).into());
            let ev = |v: serde_json::Value| {
                format!("event: {}\ndata: {v}\n\n", v["type"].as_str().unwrap_or(""))
            };
            let start = ev(
                serde_json::json!({"type":"message_start","message":{"model":"stand-in-model"}}),
            );
            let body_text = String::from_utf8_lossy(&body).to_string();
            let reply = if body_text.contains("You turn a request in plain English") {
                // The command bar: one command.
                start
                    + &ev(
                        serde_json::json!({"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}),
                    )
                    + &ev(
                        serde_json::json!({"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"echo from-the-bar"}}),
                    )
                    + &ev(
                        serde_json::json!({"type":"message_delta","delta":{"stop_reason":"end_turn"}}),
                    )
            } else if n == 0 {
                start
                    + &ev(
                        serde_json::json!({"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"toolu_smoke","name":"type_in_terminal","input":{}}}),
                    )
                    + &ev(
                        serde_json::json!({"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":"{\"command\":\"echo from-ai\",\"run\":true}"}}),
                    )
                    + &ev(serde_json::json!({"type":"content_block_stop","index":0}))
                    + &ev(
                        serde_json::json!({"type":"message_delta","delta":{"stop_reason":"tool_use"}}),
                    )
            } else {
                start
                    + &ev(
                        serde_json::json!({"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}),
                    )
                    + &ev(
                        serde_json::json!({"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"It printed from-ai."}}),
                    )
                    + &ev(
                        serde_json::json!({"type":"message_delta","delta":{"stop_reason":"end_turn"}}),
                    )
            };
            let _ = write!(
                s,
                "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{reply}",
                reply.len()
            );
        }
    });
    base
}

fn save_png(path: &std::path::Path, img: &egui::ColorImage) -> std::io::Result<()> {
    let file = std::io::BufWriter::new(std::fs::File::create(path)?);
    let mut enc = png::Encoder::new(file, img.size[0] as u32, img.size[1] as u32);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut w = enc.write_header().map_err(std::io::Error::other)?;
    let bytes: Vec<u8> = img.pixels.iter().flat_map(|p| p.to_array()).collect();
    w.write_image_data(&bytes).map_err(std::io::Error::other)
}
