//! Self-test (JAMALTERM_SMOKE=<dir>): the real app types into its own shell,
//! presses keys as a person would, checks what it sees, saves screenshots and
//! writes report.json, then quits. CI runs it on a Mac and on Linux (xvfb).

use crate::app::Tab;
use serde::Serialize;
use std::path::PathBuf;
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
        name: "⌘T opens a tab",
        act: |s, _, _, _| s.press(egui::Key::T, cmd_mods()),
        done: |_, tabs, _| (tabs.len() == 2).then(|| (true, "2 tabs".into())),
        limit: Duration::from_secs(5),
    },
    Step {
        name: "the session is saved (tabs, folders, output)",
        act: |s, _, _, _| s.save_session = true,
        done: |s, _, _| {
            let w = crate::restore::load(&s.dir.join("session.json"));
            let groups = w.first()?.groups();
            (groups.len() == 2).then(|| {
                let crate::restore::SavedNode::Leaf { tab } = &groups[0] else {
                    return (false, "the first tab was saved split".into());
                };
                (
                    tab.output.contains("smoke-ok") && !tab.cwd.is_empty(),
                    format!("2 tabs, {} bytes of output", tab.output.len()),
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
        name: "the saved tabs come back with their output",
        act: |_, _, _, _| {},
        done: |_, tabs, _| {
            let first = tabs.first()?;
            let s = first.pane().session.screen.lock().unwrap();
            let text: Vec<String> = (0..s.abs_cursor_line()).map(|l| s.line_text(l)).collect();
            let back = text.iter().any(|l| l.contains("smoke-ok"));
            let marked = text
                .iter()
                .any(|l| l.contains("Restored from the last session"));
            (back && marked && s.prompt_seen).then(|| {
                (
                    tabs.len() == 2,
                    format!("{} tabs, the output and a new prompt", tabs.len()),
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
        Some(Smoke {
            keys: vec![],
            find_status: None,
            menu_bar: None,
            save_session: false,
            dir,
            started: Instant::now(),
            step: 0,
            acted: false,
            at: Instant::now(),
            checks: vec![],
        })
    }

    /// Saves a screenshot of the next frame as <dir>/<name>.png.
    fn shot(&self, ctx: &egui::Context, name: &str) {
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
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
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

fn save_png(path: &std::path::Path, img: &egui::ColorImage) -> std::io::Result<()> {
    let file = std::io::BufWriter::new(std::fs::File::create(path)?);
    let mut enc = png::Encoder::new(file, img.size[0] as u32, img.size[1] as u32);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut w = enc.write_header().map_err(std::io::Error::other)?;
    let bytes: Vec<u8> = img.pixels.iter().flat_map(|p| p.to_array()).collect();
    w.write_image_data(&bytes).map_err(std::io::Error::other)
}
