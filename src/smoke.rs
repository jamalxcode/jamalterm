//! Self-test (JAMALTERM_SMOKE=<dir>): the real app types a few commands into
//! its own shell, opens a tab, checks what it sees and writes report.json, then
//! quits. CI runs it on a Mac and on Linux under xvfb.

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

pub struct Smoke {
    dir: PathBuf,
    started: Instant,
    step: usize,
    at: Instant,
    checks: Vec<Check>,
}

impl Smoke {
    pub fn from_env() -> Option<Smoke> {
        let dir = PathBuf::from(std::env::var_os("JAMALTERM_SMOKE")?);
        let _ = std::fs::create_dir_all(&dir);
        Some(Smoke {
            dir,
            started: Instant::now(),
            step: 0,
            at: Instant::now(),
            checks: vec![],
        })
    }

    fn check(&mut self, name: &str, ok: bool, detail: impl Into<String>) {
        self.checks.push(Check {
            name: name.into(),
            ok,
            detail: detail.into(),
        });
    }

    /// Called every frame; moves on when the last step's result is in.
    pub fn step(&mut self, ctx: &egui::Context, tabs: &[Tab], active: usize) {
        ctx.request_repaint_after(Duration::from_millis(100));
        let waited = self.at.elapsed();
        let tab = &tabs[active];
        let blocks = || tab.session.screen.lock().unwrap().commands.blocks.clone();
        let next = |s: &mut Smoke| {
            s.step += 1;
            s.at = Instant::now();
        };
        match self.step {
            0 => {
                // The first prompt: the shell started with its hooks.
                let prompt = tab.session.screen.lock().unwrap().prompt_seen;
                if prompt || waited > Duration::from_secs(15) {
                    let ms = self.started.elapsed().as_millis();
                    self.check("first prompt", prompt, format!("{ms} ms after start"));
                    tab.session.write(b"echo smoke-ok\r");
                    next(self);
                }
            }
            1 => {
                let b = blocks();
                let done = b
                    .iter()
                    .any(|b| b.command == "echo smoke-ok" && b.exit == Some(0));
                if done || waited > Duration::from_secs(10) {
                    self.check(
                        "a command and its ✓",
                        done,
                        format!("{} commands seen", b.len()),
                    );
                    tab.session.write(b"false\r");
                    next(self);
                }
            }
            2 => {
                let failed = blocks()
                    .iter()
                    .any(|b| b.command == "false" && b.exit == Some(1));
                if failed || waited > Duration::from_secs(10) {
                    self.check("a failed command and its ✕", failed, "");
                    next(self);
                }
            }
            3 => {
                // A moment to look (and screenshot) before quitting.
                if waited > Duration::from_secs(3) {
                    next(self);
                }
            }
            _ => {
                let report = serde_json::json!({
                    "ok": self.checks.iter().all(|c| c.ok),
                    "checks": self.checks,
                    "tabs": tabs.len(),
                });
                let _ = std::fs::write(
                    self.dir.join("report.json"),
                    serde_json::to_vec_pretty(&report).unwrap(),
                );
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }
}
