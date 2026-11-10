//! A small, private problem log, crash reports and Help → Copy Diagnostics.
//! Nothing here is ever sent anywhere: the report is shown in full and
//! copied only when you press Copy, to paste wherever you choose.
//!
//! Only problems are logged (a shell didn't start, an AI provider refused,
//! an update failed…). Commands, terminal output, AI questions and answers,
//! and keys are never logged. Every line is scrubbed first: things that look
//! like secrets are hidden, your home folder becomes ~, and email addresses
//! and this computer's name are removed. 256 KB, plus one older file.
//!
//! A crash (a panic) writes `crash.log`; the next start says jamalterm quit
//! unexpectedly and offers the diagnostics. A clean quit folds panics from
//! background threads into the problem log instead.

use regex::Regex;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex, OnceLock};

pub const MAX_BYTES: u64 = 256 * 1024;
const MAX_LINE: usize = 400;

pub struct Log {
    pub path: PathBuf,
    enabled: bool,
    home: String,
    host: String,
}

static LOG: OnceLock<Mutex<Log>> = OnceLock::new();

static EMAIL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[A-Za-z0-9._%+\-]+@[A-Za-z0-9.\-]+\.[A-Za-z]{2,}").unwrap());
static SPACE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s+").unwrap());

fn hostname() -> String {
    let mut buf = [0u8; 256];
    let n = unsafe { libc::gethostname(buf.as_mut_ptr().cast(), buf.len()) };
    if n != 0 {
        return String::new();
    }
    let end = buf.iter().position(|b| *b == 0).unwrap_or(buf.len());
    String::from_utf8_lossy(&buf[..end])
        .trim_end_matches(".local")
        .to_string()
}

impl Log {
    pub fn new(path: PathBuf, enabled: bool) -> Log {
        Log {
            path,
            enabled,
            home: dirs::home_dir()
                .map(|h| h.display().to_string())
                .unwrap_or_default(),
            host: hostname(),
        }
    }

    /// Hides secrets, the home folder, email addresses and the computer's
    /// name in a text, keeping its lines.
    pub fn scrub_text(&self, s: &str) -> String {
        let (mut s, _) = crate::ai::redact::redact(s);
        if self.home.len() > 1 {
            s = s.replace(&self.home, "~");
        }
        s = EMAIL.replace_all(&s, "[email]").into_owned();
        if self.host.len() >= 3
            && let Ok(re) = Regex::new(&format!("(?i){}", regex::escape(&self.host)))
        {
            s = re.replace_all(&s, "[this computer]").into_owned();
        }
        s
    }

    /// One line, scrubbed and cut short.
    pub fn scrub(&self, s: &str) -> String {
        let one = SPACE.replace_all(s, " ");
        let s = self.scrub_text(one.trim());
        let chars: Vec<char> = s.chars().collect();
        if chars.len() > MAX_LINE {
            chars[..MAX_LINE].iter().collect::<String>() + "…"
        } else {
            s
        }
    }

    /// Records one problem. `area` is a short word: "shell", "ai", "update"…
    pub fn problem(&self, area: &str, msg: &str) {
        if !self.enabled {
            return;
        }
        let line = format!("{}  {}: {}\n", utc_now(), self.scrub(area), self.scrub(msg));
        if let Some(dir) = self.path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if std::fs::metadata(&self.path).is_ok_and(|m| m.len() + line.len() as u64 > MAX_BYTES) {
            let _ = std::fs::rename(&self.path, older(&self.path));
        }
        use std::os::unix::fs::OpenOptionsExt;
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .mode(0o600)
            .open(&self.path)
        {
            let _ = f.write_all(line.as_bytes());
        }
    }

    /// Turning the log off deletes it.
    pub fn set_enabled(&mut self, on: bool) {
        self.enabled = on;
        if !on {
            let _ = std::fs::remove_file(&self.path);
            let _ = std::fs::remove_file(older(&self.path));
        }
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    /// The newest `n` lines, oldest first.
    pub fn recent(&self, n: usize) -> Vec<String> {
        let mut lines: Vec<String> = [older(&self.path), self.path.clone()]
            .iter()
            .filter_map(|p| std::fs::read_to_string(p).ok())
            .flat_map(|t| {
                t.lines()
                    .map(str::trim)
                    .filter(|l| !l.is_empty())
                    .map(str::to_string)
                    .collect::<Vec<_>>()
            })
            .collect();
        let skip = lines.len().saturating_sub(n);
        lines.drain(..skip);
        lines
    }
}

fn older(p: &Path) -> PathBuf {
    let mut s = p.as_os_str().to_owned();
    s.push(".1");
    PathBuf::from(s)
}

/// "2026-11-10 14:05:09" (UTC).
pub fn utc_now() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let (y, m, d) = civil(days);
    format!(
        "{y:04}-{m:02}-{d:02} {:02}:{:02}:{:02}",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

/// Days since 1970-01-01 → (year, month, day) (Howard Hinnant's algorithm).
fn civil(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

/// Sets up the problem log (next to settings.json).
pub fn init(settings_dir: &Path, enabled: bool) {
    let _ = LOG.set(Mutex::new(Log::new(
        settings_dir.join("problems.log"),
        enabled,
    )));
}

fn with<T>(f: impl FnOnce(&mut Log) -> T) -> Option<T> {
    LOG.get()
        .map(|l| f(&mut l.lock().unwrap_or_else(|e| e.into_inner())))
}

/// Records a problem in the private log (when it's on).
pub fn problem(area: &str, msg: &str) {
    with(|l| l.problem(area, msg));
}

pub fn set_enabled(on: bool) {
    with(|l| l.set_enabled(on));
}

pub fn scrub_text(s: &str) -> String {
    with(|l| l.scrub_text(s)).unwrap_or_else(|| crate::ai::redact::redact(s).0)
}

pub fn recent(n: usize) -> Vec<String> {
    with(|l| l.recent(n)).unwrap_or_default()
}

pub fn enabled() -> bool {
    with(|l| l.enabled()).unwrap_or(false)
}

// ---- Crashes ------------------------------------------------------------

static CRASH_FILE: OnceLock<PathBuf> = OnceLock::new();

/// Reads a crash from last time (scrubbed summary), then makes panics write
/// to crash.log for this run.
pub fn watch_crashes(settings_dir: &Path) -> Option<String> {
    let path = settings_dir.join("crash.log");
    let last = std::fs::read_to_string(&path)
        .ok()
        .filter(|t| !t.trim().is_empty())
        .map(|t| summary(&scrub_text(&t)));
    let _ = std::fs::remove_file(&path);
    if let Some(l) = &last {
        problem(
            "crash",
            &format!(
                "jamalterm quit unexpectedly last time: {}",
                l.lines().next().unwrap_or("")
            ),
        );
    }
    let _ = CRASH_FILE.set(path);
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if let Some(p) = CRASH_FILE.get() {
            let bt = std::backtrace::Backtrace::force_capture();
            let thread = std::thread::current().name().unwrap_or("?").to_string();
            let text = format!("{}  panic in thread {thread}: {info}\n{bt}\n", utc_now());
            use std::os::unix::fs::OpenOptionsExt;
            if let Ok(mut f) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .mode(0o600)
                .open(p)
            {
                let _ = f.write_all(text.as_bytes());
            }
        }
        default(info);
    }));
    last
}

/// A clean quit: panics from background threads (the app went on) go into
/// the problem log, and crash.log is removed.
pub fn quit_cleanly() {
    if let Some(p) = CRASH_FILE.get()
        && let Ok(t) = std::fs::read_to_string(p)
    {
        let first = t.lines().next().unwrap_or("").to_string();
        problem("panic", &first);
        let _ = std::fs::remove_file(p);
    }
}

/// The panic message and the first frames: enough to fix it, short enough
/// to read.
pub fn summary(trace: &str) -> String {
    trace
        .trim()
        .lines()
        .filter(|l| !l.trim_start().starts_with("at /rustc/"))
        .take(30)
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn log(dir: &Path) -> Log {
        Log {
            path: dir.join("problems.log"),
            enabled: true,
            home: "/Users/jamal".into(),
            host: "Jamals-MacBook".into(),
        }
    }

    #[test]
    fn lines_are_scrubbed() {
        let dir = tempfile::tempdir().unwrap();
        let l = log(dir.path());
        l.problem(
            "ai",
            "refused key sk-ant-api03-abcdefghijklmnopqrstuvwxyz for me@example.com\nin /Users/jamal/code on jamals-macbook",
        );
        let r = l.recent(10);
        assert_eq!(r.len(), 1, "one line");
        let line = &r[0];
        assert!(
            line.contains(
                "ai: refused key [hidden secret] for [email] in ~/code on [this computer]"
            ),
            "{line}"
        );
        assert!(!line.contains("jamal"));
        let long = "x".repeat(1000);
        assert!(l.scrub(&long).chars().count() <= MAX_LINE + 1);
    }

    #[test]
    fn rolls_over_and_turns_off() {
        let dir = tempfile::tempdir().unwrap();
        let mut l = log(dir.path());
        let big = "y".repeat(390);
        for _ in 0..800 {
            l.problem("shell", &big);
        }
        assert!(std::fs::metadata(&l.path).unwrap().len() <= MAX_BYTES);
        assert!(older(&l.path).exists(), "one older file");
        assert_eq!(l.recent(5).len(), 5);
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&l.path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        l.set_enabled(false);
        assert!(!l.path.exists() && !older(&l.path).exists());
        l.problem("shell", "not kept");
        assert!(!l.path.exists());
    }

    #[test]
    fn dates() {
        assert_eq!(civil(0), (1970, 1, 1));
        assert_eq!(civil(20_402), (2025, 11, 10));
        assert_eq!(civil(11_016), (2000, 2, 29));
        assert_eq!(utc_now().len(), 19);
    }

    #[test]
    fn crash_summaries_are_short() {
        let t = (0..100)
            .map(|i| format!("frame {i}"))
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(summary(&t).lines().count(), 30);
    }
}
