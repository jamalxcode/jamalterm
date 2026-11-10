//! Updating jamalterm from GitHub Releases (3.x's internal/update, ported).
//!
//! It downloads the release's .zip, checks it against SHA256SUMS.txt from the
//! same release, checks the Sigstore build proof (`provenance.rs`), unpacks
//! it, checks the bundle id and code signature, then swaps the .app in place
//! (putting the old one back if anything fails). The old version is kept for
//! "Go Back to Previous Version". The caller relaunches.
//!
//! Nothing about the user is sent: the requests are plain GETs to GitHub
//! with a fixed User-Agent.

pub mod provenance;

use sha2::{Digest, Sha256};
use std::cmp::Ordering;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

pub const REPO: &str = "jamalxcode/jamalterm";
#[allow(dead_code)] // checked inside the install code; kept for callers
pub const BUNDLE_ID: &str = "com.jamalxcode.jamalterm";
pub const API_BASE: &str = "https://api.github.com";

/// This build's version (Cargo.toml).
pub fn current_version() -> &'static str {
    // A release build gets its version from scripts/build-mac.sh.
    match option_env!("JAMALTERM_VERSION") {
        Some(v) if !v.is_empty() && v != "dev" => v,
        _ => env!("CARGO_PKG_VERSION"),
    }
}

/// Which releases count: Stable only, or Beta (GitHub prereleases) too.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Channel {
    #[default]
    Stable,
    Beta,
}

/// A published version and where its files are.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Release {
    /// "3.5.0" (no "v").
    pub version: String,
    /// The release notes (Markdown).
    pub notes: String,
    /// The release page.
    pub url: String,
    zip_url: String,
    zip_name: String,
    sums_url: String,
    /// The Sigstore bundle proving GitHub built zip_name from this
    /// repository (None for releases before 3.1, which aren't installed).
    bundle_url: Option<String>,
}

// ---- HTTP ----

/// Why a download failed.
#[derive(Debug)]
pub enum FetchError {
    /// No answer (network, TLS, timeout, or the writer gave up).
    Net(String),
    /// The server answered with another status than 200.
    Status(u16),
}

impl std::fmt::Display for FetchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FetchError::Net(e) => write!(f, "{e}"),
            FetchError::Status(s) => write!(f, "the server answered {s}"),
        }
    }
}

/// A GET request, behind a trait so tests don't need the network.
pub trait Fetch {
    /// Writes the body of `url` to `out`, calling `progress(done, total)`
    /// as bytes arrive.
    fn get(
        &self,
        url: &str,
        accept: &str,
        out: &mut dyn Write,
        progress: &mut dyn FnMut(u64, Option<u64>),
    ) -> Result<(), FetchError>;
}

/// The real thing: ureq, set up like the AI panel's (the system's TLS on a
/// Mac), with the 5-minute limit 3.x had.
pub struct Http {
    agent: ureq::Agent,
}

impl Default for Http {
    fn default() -> Self {
        Http {
            agent: crate::ai::agent(),
        }
    }
}

impl Fetch for Http {
    fn get(
        &self,
        url: &str,
        accept: &str,
        out: &mut dyn Write,
        progress: &mut dyn FnMut(u64, Option<u64>),
    ) -> Result<(), FetchError> {
        let mut resp = self
            .agent
            .get(url)
            .header("User-Agent", "jamalterm-updater")
            .header("Accept", accept)
            .config()
            .timeout_global(Some(Duration::from_secs(300)))
            .build()
            .call()
            .map_err(|e| FetchError::Net(e.to_string()))?;
        if resp.status() != 200 {
            return Err(FetchError::Status(resp.status().as_u16()));
        }
        let total = resp.body().content_length();
        let mut r = resp.body_mut().with_config().limit(u64::MAX).reader();
        copy_with_progress(&mut r, out, total, progress).map_err(|e| FetchError::Net(e.to_string()))
    }
}

fn copy_with_progress(
    r: &mut dyn io::Read,
    out: &mut dyn Write,
    total: Option<u64>,
    progress: &mut dyn FnMut(u64, Option<u64>),
) -> io::Result<()> {
    let mut buf = vec![0u8; 64 * 1024];
    let mut done = 0u64;
    loop {
        let n = match r.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        };
        out.write_all(&buf[..n])?;
        done += n as u64;
        progress(done, total);
    }
    out.flush()
}

/// A writer that refuses more than `limit` bytes.
struct Limited {
    buf: Vec<u8>,
    limit: usize,
}

impl Write for Limited {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        if self.buf.len() + b.len() > self.limit {
            return Err(io::Error::other("the answer is too big"));
        }
        self.buf.extend_from_slice(b);
        Ok(b.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// A file writer that also computes the SHA-256 of what it wrote.
struct Hashing {
    file: std::fs::File,
    hash: Sha256,
}

impl Write for Hashing {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        let n = self.file.write(b)?;
        self.hash.update(&b[..n]);
        Ok(n)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}

// ---- The updater ----

/// Checks the provenance of a download: (bundle JSON, SHA-256 hex).
pub type Verifier = Box<dyn Fn(&[u8], &str) -> Result<(), String> + Send + Sync>;

/// Everything the updater needs; `Updater::new` sets it up for real use.
pub struct Updater<F: Fetch = Http> {
    pub fetch: F,
    /// GitHub's API (changed in tests).
    pub api_base: String,
    /// Where the replaced version is kept for going back (None: not kept).
    pub keep_previous_in: Option<PathBuf>,
    /// The Sigstore check (replaced in tests that serve fake releases).
    pub verify: Verifier,
}

impl Updater<Http> {
    /// The real updater. `keep_previous_in` is a folder in the app's
    /// settings folder, e.g. `~/Library/Application Support/jamalterm/previous`.
    pub fn new(keep_previous_in: Option<PathBuf>) -> Self {
        Updater::with_fetch(Http::default(), keep_previous_in)
    }
}

impl<F: Fetch> Updater<F> {
    pub fn with_fetch(fetch: F, keep_previous_in: Option<PathBuf>) -> Self {
        Updater {
            fetch,
            api_base: API_BASE.to_string(),
            keep_previous_in,
            verify: Box::new(provenance::verify),
        }
    }

    fn get_bytes(&self, url: &str, accept: &str, limit: usize) -> Result<Vec<u8>, FetchError> {
        let mut out = Limited {
            buf: Vec::new(),
            limit,
        };
        self.fetch.get(url, accept, &mut out, &mut |_, _| {})?;
        Ok(out.buf)
    }

    /// The newest release on `channel` (drafts never count).
    pub fn latest(&self, channel: Channel) -> Result<Release, String> {
        let beta = channel == Channel::Beta;
        let path = if beta {
            "/releases?per_page=20"
        } else {
            "/releases/latest"
        };
        let url = format!("{}/repos/{REPO}{path}", self.api_base);
        let body = self
            .get_bytes(&url, "application/vnd.github+json", 8 << 20)
            .map_err(|e| match e {
                FetchError::Net(e) => format!("couldn't reach GitHub: {e}"),
                FetchError::Status(s) => format!("GitHub answered {s}"),
            })?;
        let list: Vec<GhRelease> = if beta {
            serde_json::from_slice(&body)
        } else {
            serde_json::from_slice(&body).map(|one| vec![one])
        }
        .map_err(|e| format!("GitHub's answer didn't make sense: {e}"))?;
        list.into_iter()
            .filter(|r| !r.draft && (beta || !r.prerelease))
            .reduce(|best, r| {
                if compare(&r.tag_name, &best.tag_name) == Ordering::Greater {
                    r
                } else {
                    best
                }
            })
            .ok_or_else(|| "no releases yet".to_string())?
            .release()
    }

    /// A release newer than `current` on `channel`, or None when up to date.
    pub fn check(&self, channel: Channel, current: &str) -> Result<Option<Release>, String> {
        let rel = self.latest(channel)?;
        Ok(newer(&rel.version, current).then_some(rel))
    }

    /// Downloads `rel` next to `app` and checks it: checksum, build proof,
    /// that it is a jamalterm app and its code signature. Nothing is
    /// changed yet; drop the result to throw it away.
    pub fn download(
        &self,
        rel: &Release,
        app: &Path,
        progress: &mut dyn FnMut(u64, Option<u64>),
    ) -> Result<Staged, String> {
        let parent = app.parent().ok_or("the app has no folder")?;
        // Stage next to the app so the final swap is a rename on the same disk.
        let mut stage = Staged::make(parent, "update").map_err(|e| {
            format!(
                "can't write to {} ({e}); download the new version from the release page instead",
                parent.display()
            )
        })?;

        let want = self.expected_sum(rel)?;
        let zip_path = stage.dir.join(&rel.zip_name);
        let file = std::fs::File::create(&zip_path).map_err(|e| format!("download failed: {e}"))?;
        let mut out = Hashing {
            file,
            hash: Sha256::new(),
        };
        self.fetch
            .get(&rel.zip_url, "application/octet-stream", &mut out, progress)
            .map_err(|e| format!("download failed: {e}"))?;
        let got = provenance::to_hex(&out.hash.finalize());
        if !got.eq_ignore_ascii_case(&want) {
            return Err("the download is damaged (checksum mismatch); nothing was changed".into());
        }
        self.check_provenance(rel, &got)?;

        unzip(&zip_path, &stage.dir).map_err(|e| format!("couldn't unpack the update: {e}"))?;
        let _ = std::fs::remove_file(&zip_path);
        verify_app(&stage.new_app)?;
        stage.version = rel.version.clone();
        Ok(stage)
    }

    /// Download, check and install in one go (3.x's `Install`).
    #[allow(dead_code)] // download and install in one call (3.x's Install)
    pub fn install(
        &self,
        rel: &Release,
        app: &Path,
        progress: &mut dyn FnMut(u64, Option<u64>),
    ) -> Result<(), String> {
        let staged = self.download(rel, app, progress)?;
        self.install_staged(staged, app)
    }

    /// Swaps a checked download in place of `app`: old aside, new in, and
    /// the old one back if that fails. The old version is then kept for
    /// going back. The running process keeps its open files, so moving its
    /// own bundle is safe; relaunch to run the new version.
    pub fn install_staged(&self, staged: Staged, app: &Path) -> Result<(), String> {
        let backup = staged.dir.join("previous.app");
        std::fs::rename(app, &backup)
            .map_err(|e| format!("couldn't move the old version aside: {e}"))?;
        if let Err(e) = std::fs::rename(&staged.new_app, app) {
            if let Err(re) = std::fs::rename(&backup, app) {
                return Err(format!(
                    "update failed ({e}) and the old app couldn't be restored ({re}); reinstall from the release page"
                ));
            }
            return Err(format!("couldn't move the new version in place: {e}"));
        }
        self.keep_previous(&backup);
        #[cfg(target_os = "macos")]
        {
            // Downloaded by us, checked by us: no "downloaded from the
            // internet" question on the next start.
            let _ = std::process::Command::new("/usr/bin/xattr")
                .args(["-dr", "com.apple.quarantine"])
                .arg(app)
                .status();
        }
        Ok(())
    }

    fn keep_previous(&self, app: &Path) {
        let Some(dir) = &self.keep_previous_in else {
            return;
        };
        let dst = dir.join("jamalterm.app");
        let _ = std::fs::remove_dir_all(&dst);
        if std::fs::create_dir_all(dir).is_ok() {
            // On another disk this fails: then it just isn't kept.
            let _ = std::fs::rename(app, dst);
        }
    }

    /// The kept older version, if there is one.
    pub fn previous_app(&self) -> Option<PathBuf> {
        let p = self.keep_previous_in.as_ref()?.join("jamalterm.app");
        p.join("Contents/MacOS/jamalterm").is_file().then_some(p)
    }

    /// Swaps the kept older version back in. The version being replaced is
    /// kept in turn, so going back again goes forward.
    pub fn revert(&self, app: &Path) -> Result<(), String> {
        let prev = self
            .previous_app()
            .ok_or("there's no earlier version to go back to")?;
        verify_app(&prev)?;
        let parent = app.parent().ok_or("the app has no folder")?;
        let stage = Staged::make(parent, "revert")
            .map_err(|e| format!("can't write to {}: {e}", parent.display()))?;
        let current = stage.dir.join("current.app");
        std::fs::rename(app, &current)
            .map_err(|e| format!("couldn't move the current version aside: {e}"))?;
        if let Err(e) = std::fs::rename(&prev, app) {
            // Another disk: copy instead.
            if copy_app(&prev, app).is_err() {
                let _ = std::fs::remove_dir_all(app);
                let _ = std::fs::rename(&current, app);
                return Err(format!("couldn't put the earlier version in place: {e}"));
            }
            let _ = std::fs::remove_dir_all(&prev);
        }
        self.keep_previous(&current);
        Ok(())
    }

    fn expected_sum(&self, rel: &Release) -> Result<String, String> {
        let body = self
            .get_bytes(&rel.sums_url, "text/plain", 1 << 20)
            .map_err(|e| format!("couldn't get the checksums: {e}"))?;
        sum_for(&String::from_utf8_lossy(&body), &rel.zip_name)
            .ok_or_else(|| "the release has no checksum for the download".to_string())
    }

    fn check_provenance(&self, rel: &Release, sha256_hex: &str) -> Result<(), String> {
        let Some(url) = &rel.bundle_url else {
            return Err("this release has no proof that GitHub built it, so it wasn't installed; nothing was changed. You can download it from the release page instead".into());
        };
        let data = self
            .get_bytes(url, "application/json", 4 << 20)
            .map_err(|e| format!("couldn't get the build proof: {e}"))?;
        (self.verify)(&data, sha256_hex).map_err(|e| format!("{e}; nothing was changed"))
    }
}

/// The checksum for `name` in a SHA256SUMS.txt ("<hex>  name" or "<hex> *name").
fn sum_for(sums: &str, name: &str) -> Option<String> {
    sums.lines().find_map(|l| {
        let mut f = l.split_whitespace();
        let (sum, file) = (f.next()?, f.next()?);
        (f.next().is_none() && file.trim_start_matches('*') == name && sum.len() == 64)
            .then(|| sum.to_string())
    })
}

/// A checked download waiting to be installed. Its folder (next to the app)
/// is removed when this is dropped.
pub struct Staged {
    dir: PathBuf,
    new_app: PathBuf,
    /// The version it holds.
    pub version: String,
}

impl Staged {
    fn make(parent: &Path, what: &str) -> io::Result<Staged> {
        let nanos = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let dir = parent.join(format!(".jamalterm-{what}-{}-{nanos}", std::process::id()));
        std::fs::create_dir(&dir)?;
        Ok(Staged {
            new_app: dir.join("jamalterm.app"),
            dir,
            version: String::new(),
        })
    }
}

impl Drop for Staged {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

#[derive(serde::Deserialize)]
struct GhRelease {
    tag_name: String,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    html_url: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    assets: Vec<GhAsset>,
}

#[derive(serde::Deserialize)]
struct GhAsset {
    name: String,
    browser_download_url: String,
}

impl GhRelease {
    fn release(self) -> Result<Release, String> {
        let mut rel = Release {
            version: self.tag_name.trim_start_matches('v').to_string(),
            notes: self.body.unwrap_or_default(),
            url: self.html_url,
            zip_url: String::new(),
            zip_name: String::new(),
            sums_url: String::new(),
            bundle_url: None,
        };
        for a in self.assets {
            if a.name.ends_with("-macos.zip") {
                rel.zip_url = a.browser_download_url;
                rel.zip_name = a.name;
            } else if a.name.ends_with("-macos.zip.sigstore.json") {
                rel.bundle_url = Some(a.browser_download_url);
            } else if a.name == "SHA256SUMS.txt" {
                rel.sums_url = a.browser_download_url;
            }
        }
        if rel.zip_url.is_empty() || rel.sums_url.is_empty() {
            return Err("the latest release has no macOS download yet".into());
        }
        Ok(rel)
    }
}

// ---- Versions ----

/// Whether version `a` is newer than `b` ("1.2.0" > "1.1.9"). Development
/// builds ("dev", "0.0.0-dev.abc") are older than any release.
pub fn newer(a: &str, b: &str) -> bool {
    compare(a, b) == Ordering::Greater
}

/// Orders versions: 3.1.0 > 3.1.0-beta.2 > 3.1.0-beta.1 > 3.0.9. A "-dev"
/// build counts as the version it's built from (4.0.0-dev = 4.0.0).
pub fn compare(a: &str, b: &str) -> Ordering {
    let ord = numbers(a).cmp(&numbers(b));
    if ord != Ordering::Equal {
        return ord;
    }
    let (qa, qb) = (pre(a), pre(b));
    match (qa.is_empty(), qb.is_empty()) {
        (true, true) => return Ordering::Equal,
        (true, false) => return Ordering::Greater, // a release beats its test versions
        (false, true) => return Ordering::Less,
        _ => {}
    }
    pre_num(qa).cmp(&pre_num(qb)).then_with(|| qa.cmp(qb))
}

/// The test-version part: "beta.2" in "3.1.0-beta.2".
fn pre(v: &str) -> &str {
    let v = v.trim_start_matches('v');
    let v = v.split('+').next().unwrap_or(v);
    match v.split_once('-') {
        Some((_, p)) if p.starts_with("dev") => "",
        Some((_, p)) => p,
        None => "",
    }
}

fn pre_num(p: &str) -> i64 {
    p.rsplit('.')
        .next()
        .and_then(|n| n.parse().ok())
        .unwrap_or(-1)
}

fn numbers(v: &str) -> [u64; 3] {
    let v = v.trim_start_matches('v');
    let v = v.split(['-', '+']).next().unwrap_or(v);
    let mut out = [0; 3];
    for (i, p) in v.splitn(3, '.').enumerate() {
        match p.parse() {
            Ok(n) => out[i] = n,
            Err(_) => return [0; 3],
        }
    }
    out
}

// ---- The app on disk ----

/// The .app this program runs from, e.g. /Applications/jamalterm.app.
pub fn app_bundle() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let exe = exe.canonicalize().unwrap_or(exe);
    app_bundle_of(&exe)
}

fn app_bundle_of(exe: &Path) -> Result<PathBuf, String> {
    // .../jamalterm.app/Contents/MacOS/jamalterm
    exe.ancestors()
        .nth(3)
        .filter(|a| a.extension().is_some_and(|e| e == "app"))
        .map(Path::to_path_buf)
        .ok_or_else(|| "not running from an installed jamalterm.app (development build)".into())
}

/// An app bundle's version (CFBundleShortVersionString), on a Mac.
#[allow(dead_code)] // an installed app's version (Mac)
pub fn app_version(app: &Path) -> Option<String> {
    plist_value(app, "CFBundleShortVersionString")
}

#[cfg(target_os = "macos")]
fn plist_value(app: &Path, key: &str) -> Option<String> {
    let out = std::process::Command::new("/usr/libexec/PlistBuddy")
        .args(["-c", &format!("Print :{key}")])
        .arg(app.join("Contents/Info.plist"))
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

#[cfg(not(target_os = "macos"))]
#[allow(dead_code)]
fn plist_value(_app: &Path, _key: &str) -> Option<String> {
    None
}

/// Checks an unpacked app is really jamalterm and intact.
fn verify_app(app: &Path) -> Result<(), String> {
    if !app.join("Contents/MacOS/jamalterm").is_file() {
        return Err("the update doesn't contain jamalterm.app".into());
    }
    #[cfg(target_os = "macos")]
    {
        if plist_value(app, "CFBundleIdentifier").as_deref() != Some(BUNDLE_ID) {
            return Err("the update isn't a jamalterm app; nothing was changed".into());
        }
        let out = std::process::Command::new("/usr/bin/codesign")
            .args(["--verify", "--deep", "--strict"])
            .arg(app)
            .output()
            .map_err(|e| format!("couldn't check the update's code signature: {e}"))?;
        if !out.status.success() {
            return Err(format!(
                "the update's code signature is broken; nothing was changed ({})",
                String::from_utf8_lossy(&out.stderr).trim()
            ));
        }
    }
    Ok(())
}

/// Unpacks the zip into `dest`. On a Mac with ditto, which keeps
/// permissions, symlinks and extended attributes exactly (as 3.x did).
#[cfg(target_os = "macos")]
fn unzip(zip: &Path, dest: &Path) -> Result<(), String> {
    let out = std::process::Command::new("/usr/bin/ditto")
        .args(["-x", "-k"])
        .arg(zip)
        .arg(dest)
        .output()
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    Ok(())
}

/// Elsewhere (tests on Linux) with the zip crate, which refuses paths that
/// leave `dest`.
#[cfg(not(target_os = "macos"))]
fn unzip(zip: &Path, dest: &Path) -> Result<(), String> {
    let f = std::fs::File::open(zip).map_err(|e| e.to_string())?;
    let mut a = zip::ZipArchive::new(f).map_err(|e| e.to_string())?;
    a.extract(dest).map_err(|e| e.to_string())
}

#[cfg(target_os = "macos")]
fn copy_app(from: &Path, to: &Path) -> Result<(), String> {
    let ok = std::process::Command::new("/usr/bin/ditto")
        .arg(from)
        .arg(to)
        .status()
        .is_ok_and(|s| s.success());
    if ok {
        Ok(())
    } else {
        Err("ditto failed".into())
    }
}

#[cfg(not(target_os = "macos"))]
fn copy_app(_from: &Path, _to: &Path) -> Result<(), String> {
    Err("copying an app is only done on a Mac".into())
}

/// Starts the app again a moment after this process exits (quit right
/// after calling this).
pub fn relaunch(app: &Path) -> Result<(), String> {
    if !cfg!(target_os = "macos") {
        return Err("relaunch is only supported on macOS".into());
    }
    std::process::Command::new("/bin/sh")
        .args(["-c", r#"sleep 1; /usr/bin/open -n "$0""#])
        .arg(app)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("couldn't start the new version: {e}"))
}

// ---- What Settings → Updates shows ----

/// Where the updater stands, for Settings → Updates.
#[derive(Clone, Debug, PartialEq)]
pub struct Status {
    /// The running version.
    pub version: String,
    pub channel: Channel,
    pub last_checked: Option<SystemTime>,
    pub state: State,
}

#[derive(Clone, Debug, PartialEq)]
pub enum State {
    /// Not checked since the app started.
    Idle,
    Checking,
    UpToDate,
    Available(Release),
    Downloading {
        done: u64,
        total: Option<u64>,
    },
    /// Installed; it runs after a relaunch.
    Installed {
        version: String,
    },
    Failed(String),
}

impl Status {
    pub fn new(channel: Channel) -> Status {
        Status {
            version: current_version().to_string(),
            channel,
            last_checked: None,
            state: State::Idle,
        }
    }

    /// Records the result of [`Updater::check`].
    pub fn checked(&mut self, result: Result<Option<Release>, String>, now: SystemTime) {
        self.last_checked = Some(now);
        self.state = match result {
            Ok(Some(rel)) => State::Available(rel),
            Ok(None) => State::UpToDate,
            Err(e) => State::Failed(e),
        };
    }

    /// One or two plain sentences: version, when it last checked, what it
    /// found (or the error).
    pub fn text(&self, now: SystemTime) -> String {
        let channel = match self.channel {
            Channel::Stable => "",
            Channel::Beta => " (Beta channel)",
        };
        let checked = match self.last_checked {
            None => "Not checked yet.".to_string(),
            Some(t) => format!(
                "Last checked {}.",
                ago(now.duration_since(t).unwrap_or_default())
            ),
        };
        let found = match &self.state {
            State::Idle => String::new(),
            State::Checking => "Checking…".into(),
            State::UpToDate => "You're up to date.".into(),
            State::Available(r) => format!("Version {} is available.", r.version),
            State::Downloading {
                done,
                total: Some(t),
            } if *t > 0 => {
                format!("Downloading… {}%", (done * 100 / t).min(100))
            }
            State::Downloading { done, .. } => format!("Downloading… {:.1} MB", *done as f64 / 1e6),
            State::Installed { version } => {
                format!("Version {version} is installed; restart jamalterm to use it.")
            }
            State::Failed(e) => format!("✕ {}", capitalize(e)),
        };
        let head = format!("jamalterm {}{channel}. {checked}", self.version);
        if found.is_empty() {
            head
        } else {
            format!("{head} {found}")
        }
    }
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().chain(c).collect(),
        None => String::new(),
    }
}

fn ago(d: Duration) -> String {
    let s = d.as_secs();
    let (n, unit) = match s {
        0..60 => return "just now".into(),
        60..3600 => (s / 60, "minute"),
        3600..86400 => (s / 3600, "hour"),
        _ => (s / 86400, "day"),
    };
    format!("{n} {unit}{} ago", if n == 1 { "" } else { "s" })
}

#[cfg(test)]
mod tests;
