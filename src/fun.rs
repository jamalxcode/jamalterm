//! The quiet extras: Match my wallpaper and Theme of the week, project and
//! host colors on tabs, the finish line, notifications' words, and "Your
//! Day in jamalterm" (program names and timings only, never arguments or
//! output, kept in a small file on this Mac; Reset clears it).

use crate::settings::HostColor;
use crate::themes::{Theme, contrast, hex};
use egui::Color32;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};

// ---- Theme of the week and Match my wallpaper --------------------------

fn days_since_epoch() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64 / 86_400)
        .unwrap_or(0)
}

/// The built-in theme for this week (it changes every Monday).
pub fn theme_of_the_week(all: &[Theme]) -> Theme {
    // 1970-01-01 was a Thursday: shift so weeks start on Monday.
    let week = (days_since_epoch() + 3).div_euclid(7);
    let mut t = all[week.rem_euclid(all.len() as i64) as usize].clone();
    t.id = "weekly";
    t
}

/// The desktop picture's theme, made once and kept (None if there's no
/// picture to read, e.g. not on a Mac).
pub fn wallpaper_theme() -> Option<Theme> {
    static MADE: LazyLock<Mutex<Option<Option<Theme>>>> = LazyLock::new(|| Mutex::new(None));
    let mut m = MADE.lock().unwrap_or_else(|e| e.into_inner());
    if m.is_none() {
        *m = Some(crate::macos::wallpaper_pixels().map(|px| from_pixels(&px)));
    }
    m.clone().flatten()
}

fn saturation(c: [u8; 3]) -> f32 {
    let max = *c.iter().max().unwrap() as f32;
    let min = *c.iter().min().unwrap() as f32;
    if max == 0.0 { 0.0 } else { (max - min) / max }
}

fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let m = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(m(a.r(), b.r()), m(a.g(), b.g()), m(a.b(), b.b()))
}

/// Lightens a color toward white until it reads on the background.
fn readable(c: Color32, bg: Color32, ratio: f32) -> Color32 {
    let mut t = 0.0;
    let mut out = c;
    while contrast(out, bg) < ratio && t < 1.0 {
        t += 0.05;
        out = mix(c, Color32::WHITE, t);
    }
    out
}

/// A dark theme from a picture's pixels: its darkest shade (darker still)
/// for the background, its most colorful ones for blue, magenta, cyan and
/// the cursor; every color raised until it passes WCAG contrast (text
/// 4.5:1, colors 3:1). A grey picture keeps the base theme's colors.
pub fn from_pixels(px: &[[u8; 3]]) -> Theme {
    let base = crate::themes::themes()[0].clone();
    let lum = |c: &[u8; 3]| 0.2126 * c[0] as f32 + 0.7152 * c[1] as f32 + 0.0722 * c[2] as f32;
    let mut by_dark = px.to_vec();
    by_dark.sort_by(|a, b| lum(a).total_cmp(&lum(b)));
    let darkest = by_dark
        .get(by_dark.len() / 20)
        .copied()
        .unwrap_or([20, 20, 28]);
    let bg = mix(
        Color32::from_rgb(darkest[0], darkest[1], darkest[2]),
        Color32::BLACK,
        0.7,
    );
    // The most colorful pixels, one per hue region.
    let mut colorful: Vec<[u8; 3]> = px
        .iter()
        .copied()
        .filter(|c| saturation(*c) > 0.35 && lum(c) > 40.0)
        .collect();
    colorful.sort_by(|a, b| (saturation(*b) * lum(b)).total_cmp(&(saturation(*a) * lum(a))));
    let mut picks: Vec<Color32> = vec![];
    for c in colorful {
        let col = Color32::from_rgb(c[0], c[1], c[2]);
        let far = picks.iter().all(|p| {
            let d = (p.r() as i32 - c[0] as i32).abs()
                + (p.g() as i32 - c[1] as i32).abs()
                + (p.b() as i32 - c[2] as i32).abs();
            d > 90
        });
        if far {
            picks.push(col);
        }
        if picks.len() == 4 {
            break;
        }
    }
    let mut ansi = base.ansi;
    let mut cursor = base.cursor;
    if let Some(c) = picks.first() {
        cursor = *c;
    }
    for (slot, c) in [4usize, 5, 6]
        .iter()
        .zip(picks.iter().skip(1).chain(picks.iter()))
    {
        ansi[*slot] = *c;
        ansi[*slot + 8] = mix(*c, Color32::WHITE, 0.25);
    }
    ansi[0] = mix(bg, Color32::WHITE, 0.12);
    for c in ansi.iter_mut().skip(1) {
        *c = readable(*c, bg, 3.0);
    }
    let tint = picks.first().copied().unwrap_or(Color32::GRAY);
    let foreground = readable(mix(Color32::from_gray(225), tint, 0.08), bg, 4.5);
    Theme {
        id: "wallpaper",
        name: "Match my wallpaper",
        light: false,
        background: bg,
        foreground,
        cursor: readable(cursor, bg, 3.0),
        selection: mix(bg, tint, 0.35),
        ansi,
    }
}

// ---- Project and host colors --------------------------------------------

static ROOTS: LazyLock<Mutex<HashMap<String, Option<PathBuf>>>> = LazyLock::new(Default::default);

/// The git repository a folder is in (the nearest folder with a .git).
pub fn repo_root(cwd: &str) -> Option<PathBuf> {
    if cwd.is_empty() {
        return None;
    }
    let mut cache = ROOTS.lock().unwrap_or_else(|e| e.into_inner());
    cache
        .entry(cwd.to_string())
        .or_insert_with(|| {
            Path::new(cwd)
                .ancestors()
                .find(|d| d.join(".git").exists())
                .map(Path::to_path_buf)
        })
        .clone()
}

/// A project's color: blue, green, yellow, magenta or cyan from the theme
/// (red is kept for failures), the same for every tab in the repository.
pub fn project_color(root: &Path, theme: &Theme) -> Color32 {
    let mut h: u32 = 2_166_136_261;
    for b in root.to_string_lossy().bytes() {
        h = (h ^ b as u32).wrapping_mul(16_777_619);
    }
    theme.ansi[[4, 2, 3, 5, 6][(h % 5) as usize]]
}

/// The host an ssh command connects to ("ssh -p 22 me@prod-1 ls" → "prod-1").
pub fn ssh_host(command: &str) -> Option<String> {
    let mut words = command.split_whitespace();
    if words.next()? != "ssh" {
        return None;
    }
    let mut skip = false;
    for w in words {
        if skip {
            skip = false;
            continue;
        }
        if let Some(opt) = w.strip_prefix('-') {
            // Options that take a value.
            skip = opt.len() == 1 && "bcDEeFIiJLlmOopQRSWw".contains(opt);
            continue;
        }
        return Some(w.rsplit('@').next().unwrap_or(w).to_string());
    }
    None
}

/// "prod-*" style patterns (only * is special).
pub fn glob(pattern: &str, text: &str) -> bool {
    let parts: Vec<&str> = pattern.split('*').collect();
    if parts.len() == 1 {
        return pattern == text;
    }
    let mut rest = text;
    for (i, p) in parts.iter().enumerate() {
        if i == 0 {
            match rest.strip_prefix(p) {
                Some(r) => rest = r,
                None => return false,
            }
        } else if i == parts.len() - 1 {
            return rest.ends_with(p);
        } else {
            match rest.find(p) {
                Some(at) => rest = &rest[at + p.len()..],
                None => return false,
            }
        }
    }
    true
}

/// A host's color from Settings (host colors win over project colors).
pub fn host_color(command: &str, hosts: &[HostColor]) -> Option<Color32> {
    let host = ssh_host(command)?;
    hosts
        .iter()
        .find(|h| glob(&h.pattern, &host))
        .map(|h| hex(&h.color))
}

// ---- Recent folders --------------------------------------------------------

/// The folders you've worked in, newest first (recent-folders.json, 0600;
/// at most 20). Open Recent Folder (⇧⌘O) lists them.
#[derive(Serialize, Deserialize, Default, Debug, PartialEq)]
pub struct Recent {
    pub folders: Vec<String>,
}

impl Recent {
    pub fn load(path: &Path) -> Recent {
        std::fs::read(path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) {
        if let Ok(data) = serde_json::to_string(self) {
            let _ = crate::restore::save(path, &data);
        }
    }

    /// A folder was visited: it goes to the top. True if the list changed.
    pub fn visit(&mut self, dir: &str) -> bool {
        if dir.is_empty() || self.folders.first().is_some_and(|f| f == dir) {
            return false;
        }
        self.folders.retain(|f| f != dir);
        self.folders.insert(0, dir.to_string());
        self.folders.truncate(20);
        true
    }
}

// ---- Finish line and notifications ---------------------------------------

/// "3m 12s", "45s", "1h 2m".
pub fn duration(secs: u64) -> String {
    match secs {
        s if s < 60 => format!("{s}s"),
        s if s < 3600 => format!("{}m {}s", s / 60, s % 60),
        s => format!("{}h {}m", s / 3600, s % 3600 / 60),
    }
}

/// The finish line card: "✓ build done in 3m 12s" (a command that took over
/// a minute and ended while you watched).
pub fn finish_line(program: &str, ok: bool, secs: u64) -> String {
    if ok {
        format!("✓ {program} done in {}", duration(secs))
    } else {
        format!("✕ {program} failed after {}", duration(secs))
    }
}

/// A notification's title and text (when you're in another app).
pub fn notification(command: &str, exit: i32, secs: u64) -> (String, String) {
    let short: String = command.chars().take(60).collect();
    if exit == 0 {
        (
            "✓ Done".into(),
            format!("{short} finished in {}", duration(secs)),
        )
    } else {
        (
            "✕ Failed".into(),
            format!("{short} failed (exit {exit}) after {}", duration(secs)),
        )
    }
}

// ---- Your Day in jamalterm ------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Run {
    /// The program's name only ("npm"), never its arguments.
    pub program: String,
    /// When it ended (seconds since 1970).
    pub at: u64,
    pub secs: u64,
    pub ok: bool,
}

#[derive(Serialize, Deserialize, Default, Debug)]
pub struct Stats {
    pub runs: Vec<Run>,
}

#[derive(Debug, PartialEq)]
pub struct Day {
    pub commands: usize,
    pub failures: usize,
    pub longest: Option<(String, u64)>,
    /// The most used programs this week, with counts.
    pub week: Vec<(String, usize)>,
}

pub fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

impl Stats {
    pub fn load(path: &Path) -> Stats {
        std::fs::read(path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) {
        if let Ok(data) = serde_json::to_vec(self) {
            let _ = crate::restore::save(path, &String::from_utf8_lossy(&data));
        }
    }

    /// Notes a finished command (its program name and how long it took);
    /// keeps a week and a day.
    pub fn record(&mut self, program: &str, secs: u64, ok: bool, now: u64) {
        self.runs.retain(|r| now.saturating_sub(r.at) < 8 * 86_400);
        self.runs.push(Run {
            program: program.to_string(),
            at: now,
            secs,
            ok,
        });
    }

    /// Today (since local midnight is approximated as the last 24 hours'
    /// calendar day in UTC offset `utc_offset`) and this week.
    pub fn day(&self, now: u64, utc_offset: i64) -> Day {
        let local = |t: u64| (t as i64 + utc_offset).div_euclid(86_400);
        let today: Vec<&Run> = self
            .runs
            .iter()
            .filter(|r| local(r.at) == local(now))
            .collect();
        let longest = today
            .iter()
            .max_by_key(|r| r.secs)
            .filter(|r| r.secs > 0)
            .map(|r| (r.program.clone(), r.secs));
        let mut counts: HashMap<&str, usize> = HashMap::new();
        for r in self
            .runs
            .iter()
            .filter(|r| now.saturating_sub(r.at) < 7 * 86_400)
        {
            *counts.entry(&r.program).or_default() += 1;
        }
        let mut week: Vec<(String, usize)> = counts
            .into_iter()
            .map(|(p, n)| (p.to_string(), n))
            .collect();
        week.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        week.truncate(5);
        Day {
            commands: today.len(),
            failures: today.iter().filter(|r| !r.ok).count(),
            longest,
            week,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check_readable(t: &Theme) {
        assert!(contrast(t.foreground, t.background) >= 4.5, "text");
        assert!(contrast(t.cursor, t.background) >= 3.0, "cursor");
        for c in &t.ansi[1..] {
            assert!(contrast(*c, t.background) >= 3.0, "{c:?}");
        }
    }

    #[test]
    fn wallpaper_themes_are_readable() {
        // A bright sunset, a dark night sky, and a grey picture.
        let bright: Vec<[u8; 3]> = (0..400)
            .map(|i| [250, (120 + i % 100) as u8, (40 + i % 60) as u8])
            .chain((0..100).map(|_| [30, 90, 200]))
            .collect();
        let dark: Vec<[u8; 3]> = (0..500)
            .map(|i| [5, 8, (20 + i % 40) as u8])
            .chain((0..50).map(|_| [180, 40, 160]))
            .collect();
        let grey: Vec<[u8; 3]> = (0..500).map(|i| [(i % 200) as u8; 3]).collect();
        for px in [bright, dark, grey] {
            let t = from_pixels(&px);
            check_readable(&t);
            assert!(!t.light);
        }
        let t = from_pixels(
            &(0..300)
                .map(|_| [200, 30, 30])
                .chain((0..300).map(|_| [20, 20, 20]))
                .collect::<Vec<_>>(),
        );
        assert!(
            t.cursor.r() > t.cursor.b(),
            "the picture's color becomes the cursor"
        );
    }

    #[test]
    fn theme_of_the_week_is_a_built_in_one() {
        let all = crate::themes::themes();
        let t = theme_of_the_week(&all);
        assert_eq!(t.id, "weekly");
        assert!(all.iter().any(|x| x.name == t.name));
    }

    #[test]
    fn project_colors() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path().join("app");
        std::fs::create_dir_all(repo.join(".git")).unwrap();
        std::fs::create_dir_all(repo.join("src/deep")).unwrap();
        let deep = repo.join("src/deep").display().to_string();
        assert_eq!(repo_root(&deep), Some(repo.clone()));
        assert_eq!(repo_root(&dir.path().display().to_string()), None);
        let theme = crate::themes::themes()[0].clone();
        let c = project_color(&repo, &theme);
        assert_ne!(c, theme.ansi[1], "never red");
        assert_eq!(
            c,
            project_color(&repo, &theme),
            "the same for the whole repo"
        );
    }

    #[test]
    fn hosts() {
        assert_eq!(ssh_host("ssh me@prod-1").as_deref(), Some("prod-1"));
        assert_eq!(
            ssh_host("ssh -p 2222 -i key db.example.com uptime").as_deref(),
            Some("db.example.com")
        );
        assert_eq!(ssh_host("ls"), None);
        assert!(
            glob("prod-*", "prod-1")
                && !glob("prod-*", "dev-1")
                && glob("*.example.com", "db.example.com")
        );
        let hosts = vec![HostColor {
            pattern: "prod-*".into(),
            color: "#ff0000".into(),
        }];
        assert_eq!(
            host_color("ssh me@prod-9", &hosts),
            Some(Color32::from_rgb(255, 0, 0))
        );
        assert_eq!(host_color("ssh dev", &hosts), None);
    }

    #[test]
    fn recent_folders() {
        let mut r = Recent::default();
        assert!(r.visit("/a") && r.visit("/b") && !r.visit("/b"));
        assert!(r.visit("/a"));
        assert_eq!(r.folders, ["/a", "/b"]);
        for i in 0..30 {
            r.visit(&format!("/x{i}"));
        }
        assert_eq!(r.folders.len(), 20);
        assert_eq!(r.folders[0], "/x29");
    }

    #[test]
    fn words() {
        assert_eq!(duration(45), "45s");
        assert_eq!(duration(192), "3m 12s");
        assert_eq!(duration(3720), "1h 2m");
        assert_eq!(finish_line("npm", true, 192), "✓ npm done in 3m 12s");
        assert_eq!(
            notification("make", 2, 70).1,
            "make failed (exit 2) after 1m 10s"
        );
    }

    #[test]
    fn your_day() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("stats.json");
        let now = 1_800_000_000;
        let mut s = Stats::default();
        s.record("npm", 300, true, now - 100);
        s.record("git", 1, true, now - 50);
        s.record("git", 2, false, now - 10);
        s.record("cargo", 9, true, now - 3 * 86_400);
        s.record("old", 1, true, now - 9 * 86_400);
        s.record("ls", 0, true, now);
        assert!(
            !s.runs.iter().any(|r| r.program == "old"),
            "older than a week: gone"
        );
        s.save(&p);
        let s = Stats::load(&p);
        let d = s.day(now, 0);
        assert_eq!(d.commands, 4);
        assert_eq!(d.failures, 1);
        assert_eq!(d.longest, Some(("npm".into(), 300)));
        assert_eq!(d.week[0], ("git".into(), 2));
        assert!(
            !std::fs::read_to_string(&p).unwrap().contains("--"),
            "names only"
        );
    }
}
