//! The updater as the app uses it: checks in the background (at start and
//! every 6 hours when automatic updates are on, or from jamalterm → Check
//! for Updates…), downloads and checks a new version (checksum, then the
//! Sigstore build proof, then the bundle id and signature: see
//! `update/`), installs it when you quit (or now, with Install and
//! Relaunch), and can go back to the previous version. After an update a
//! What's new card lists that version's changes from the CHANGELOG built in.

use crate::update::{self, Channel, State, Status, Updater};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant, SystemTime};

enum Msg {
    Checked(Result<Option<update::Release>, String>),
    Progress(u64, Option<u64>),
    Staged(Result<String, String>),
    Done(Result<String, String>),
}

pub struct Updates {
    pub status: Status,
    rx: Option<mpsc::Receiver<Msg>>,
    staged: Arc<Mutex<Option<update::Staged>>>,
    next_check: Instant,
    /// The last check was asked for (a "you're up to date" is said aloud).
    manual: bool,
    /// Where the previous version is kept.
    keep: PathBuf,
    /// Something to tell (from a manual check, or an install).
    pub say: Option<String>,
    /// Quit now: the new version was installed and relaunches.
    pub quit: bool,
}

pub fn channel(name: &str) -> Channel {
    if name == "beta" {
        Channel::Beta
    } else {
        Channel::Stable
    }
}

impl Updates {
    pub fn new(settings_dir: &Path, channel_name: &str) -> Updates {
        Updates {
            status: Status::new(channel(channel_name)),
            rx: None,
            staged: Default::default(),
            // The first automatic check waits a little after the start.
            next_check: Instant::now() + Duration::from_secs(20),
            manual: false,
            keep: settings_dir.join("previous"),
            say: None,
            quit: false,
        }
    }

    fn updater(&self) -> Updater {
        Updater::new(Some(self.keep.clone()))
    }

    pub fn busy(&self) -> bool {
        self.rx.is_some()
    }

    /// A version is downloaded, checked and waiting to be installed.
    pub fn ready(&self) -> Option<String> {
        self.staged
            .lock()
            .ok()
            .and_then(|s| s.as_ref().map(|s| s.version.clone()))
    }

    pub fn can_revert(&self) -> bool {
        self.updater().previous_app().is_some()
    }

    /// Checks GitHub for a newer version (in the background).
    pub fn check_now(&mut self, manual: bool, wake: Arc<dyn Fn() + Send + Sync>) {
        if self.busy() {
            return;
        }
        // A test build (from CI, not a release) never updates itself: every
        // release would look newer and replace what's being tested.
        if test_build(update::current_version()) {
            if manual {
                self.say = Some(format!(
                    "jamalterm {} is a test build: it doesn't update itself. Install a release to get updates.",
                    update::current_version()
                ));
            }
            self.next_check = Instant::now() + Duration::from_secs(6 * 3600);
            return;
        }
        self.manual = manual;
        self.status.state = State::Checking;
        let (tx, rx) = mpsc::channel();
        let ch = self.status.channel;
        let keep = self.keep.clone();
        std::thread::spawn(move || {
            let r = Updater::new(Some(keep)).check(ch, update::current_version());
            let _ = tx.send(Msg::Checked(r));
            wake();
        });
        self.rx = Some(rx);
        self.next_check = Instant::now() + Duration::from_secs(6 * 3600);
    }

    /// Downloads and checks a release (in the background); it's installed
    /// when you quit, or now with Install and Relaunch.
    fn download(&mut self, rel: update::Release, wake: Arc<dyn Fn() + Send + Sync>) {
        let app = match update::app_bundle() {
            Ok(a) => a,
            Err(e) => {
                self.status.state = State::Failed(e);
                return;
            }
        };
        let (tx, rx) = mpsc::channel();
        let keep = self.keep.clone();
        let staged = self.staged.clone();
        std::thread::spawn(move || {
            let u = Updater::new(Some(keep));
            let tx2 = tx.clone();
            let w2 = wake.clone();
            let mut last = Instant::now();
            let r = u.download(&rel, &app, &mut |done, total| {
                if last.elapsed() > Duration::from_millis(200) {
                    last = Instant::now();
                    let _ = tx2.send(Msg::Progress(done, total));
                    w2();
                }
            });
            let msg = match r {
                Ok(s) => {
                    let v = s.version.clone();
                    *staged.lock().unwrap_or_else(|e| e.into_inner()) = Some(s);
                    Ok(v)
                }
                Err(e) => Err(e),
            };
            let _ = tx.send(Msg::Staged(msg));
            wake();
        });
        self.status.state = State::Downloading {
            done: 0,
            total: None,
        };
        self.rx = Some(rx);
    }

    /// Takes the background work's news. `auto`: automatic updates are on
    /// (a found version is downloaded right away).
    pub fn tick(&mut self, auto: bool, wake: Arc<dyn Fn() + Send + Sync>) {
        if auto && !self.busy() && Instant::now() > self.next_check && self.ready().is_none() {
            self.check_now(false, wake.clone());
        }
        let Some(rx) = &self.rx else { return };
        let mut next = None;
        while let Ok(m) = rx.try_recv() {
            match m {
                Msg::Checked(r) => {
                    if let Err(e) = &r {
                        crate::diag::problem("update", e);
                    }
                    self.status.checked(r, SystemTime::now());
                    self.rx = None;
                    match &self.status.state {
                        State::UpToDate if self.manual => {
                            self.say = Some(format!(
                                "jamalterm {} is the newest version.",
                                update::current_version()
                            ));
                        }
                        State::Failed(e) if self.manual => self.say = Some(e.clone()),
                        State::Available(rel) if auto || self.manual => next = Some(rel.clone()),
                        _ => {}
                    }
                    break;
                }
                Msg::Progress(done, total) => {
                    self.status.state = State::Downloading { done, total }
                }
                Msg::Staged(r) => {
                    self.rx = None;
                    self.status.state = match r {
                        Ok(v) => State::Installed { version: v },
                        Err(e) => {
                            crate::diag::problem("update", &e);
                            State::Failed(e)
                        }
                    };
                    break;
                }
                Msg::Done(r) => {
                    self.rx = None;
                    match r {
                        Ok(_) => self.quit = true,
                        Err(e) => {
                            crate::diag::problem("update", &e);
                            self.say = Some(e.clone());
                            self.status.state = State::Failed(e);
                        }
                    }
                    break;
                }
            }
        }
        if let Some(rel) = next {
            self.download(rel, wake);
        }
    }

    /// Install and Relaunch: installs the checked version, then relaunches
    /// (the app quits when `quit` turns true).
    pub fn install_now(&mut self, wake: Arc<dyn Fn() + Send + Sync>) {
        let Some(staged) = self.staged.lock().ok().and_then(|mut s| s.take()) else {
            return;
        };
        let (tx, rx) = mpsc::channel();
        let keep = self.keep.clone();
        std::thread::spawn(move || {
            let r = update::app_bundle().and_then(|app| {
                let v = staged.version.clone();
                Updater::new(Some(keep)).install_staged(staged, &app)?;
                update::relaunch(&app)?;
                Ok(v)
            });
            let _ = tx.send(Msg::Done(r));
            wake();
        });
        self.rx = Some(rx);
    }

    /// Quitting: a checked version waiting is installed now (it runs next
    /// time). "Update when I quit".
    pub fn install_at_quit(&mut self) {
        let Some(staged) = self.staged.lock().ok().and_then(|mut s| s.take()) else {
            return;
        };
        if let Err(e) =
            update::app_bundle().and_then(|app| self.updater().install_staged(staged, &app))
        {
            crate::diag::problem("update", &e);
        }
    }

    /// Go Back to Previous Version: swaps in the version kept at the last
    /// update, and relaunches.
    pub fn revert(&mut self, wake: Arc<dyn Fn() + Send + Sync>) {
        let (tx, rx) = mpsc::channel();
        let keep = self.keep.clone();
        std::thread::spawn(move || {
            let r = update::app_bundle().and_then(|app| {
                Updater::new(Some(keep)).revert(&app)?;
                update::relaunch(&app)?;
                Ok(String::new())
            });
            let _ = tx.send(Msg::Done(r));
            wake();
        });
        self.rx = Some(rx);
    }
}

/// A build that isn't a release: "0.0.0-dev.abc1234" (CI) or "…-dev".
pub fn test_build(version: &str) -> bool {
    version.starts_with("0.0.0") || version.ends_with("-dev") || version.contains("-dev.")
}

/// The CHANGELOG built into the app.
const CHANGELOG: &str = include_str!("../CHANGELOG.md");

/// A version's lines from the CHANGELOG (its "## X.Y.Z" section).
pub fn changes(version: &str, changelog: &str) -> Vec<String> {
    let mut lines = vec![];
    let mut inside = false;
    for l in changelog.lines() {
        if let Some(h) = l.strip_prefix("## ") {
            if inside {
                break;
            }
            inside = h.split_whitespace().next() == Some(version);
            continue;
        }
        if inside && !l.trim().is_empty() {
            lines.push(l.trim().trim_start_matches("- ").to_string());
        }
    }
    lines
}

/// After an update: What's new (this version's changes), once. Compares
/// with the version that ran last (a small file next to the settings).
pub fn whats_new(settings_dir: &Path) -> Option<(String, Vec<String>)> {
    let file = settings_dir.join("last-version");
    let now = update::current_version();
    let before = std::fs::read_to_string(&file).ok();
    let _ = std::fs::create_dir_all(settings_dir);
    let _ = std::fs::write(&file, now);
    let before = before?;
    let before = before.trim();
    if before.is_empty() || !update::newer(now, before) {
        return None;
    }
    let lines = changes(now, CHANGELOG);
    (!lines.is_empty()).then(|| (now.to_string(), lines))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn changelog_sections() {
        let log = "# Changelog\n\n## 4.1.0\n\n- Faster\n- Kinder\n\n## 4.0.0\n\n- Rust\n";
        assert_eq!(changes("4.1.0", log), ["Faster", "Kinder"]);
        assert_eq!(changes("4.0.0", log), ["Rust"]);
        assert!(changes("9.9.9", log).is_empty());
        assert!(
            !changes("3.5.0", CHANGELOG).is_empty(),
            "the built-in one has 3.5.0"
        );
    }

    #[test]
    fn whats_new_shows_once_after_an_update() {
        let dir = tempfile::tempdir().unwrap();
        assert!(whats_new(dir.path()).is_none(), "first start: nothing");
        std::fs::write(dir.path().join("last-version"), "3.5.0").unwrap();
        // 4.0.0-dev is newer than 3.5.0; its section may be empty while in
        // progress, so only "at most once" is checked here.
        let _ = whats_new(dir.path());
        assert!(whats_new(dir.path()).is_none(), "once");
    }

    #[test]
    fn test_builds_never_update() {
        assert!(test_build("0.0.0-dev.abc1234"));
        assert!(test_build("4.0.0-dev"));
        assert!(!test_build("4.0.0"));
        assert!(!test_build("4.0.0-beta.1"));
    }

    #[test]
    fn channels() {
        assert_eq!(channel("beta"), Channel::Beta);
        assert_eq!(channel("stable"), Channel::Stable);
        assert_eq!(channel(""), Channel::Stable);
    }
}
