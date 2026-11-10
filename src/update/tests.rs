//! The updater's tests (3.x's update_test.go, ported). Releases are served
//! by a small local HTTP server, so the real ureq path is used.

use super::*;
use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

#[test]
fn newer_versions() {
    let cases = [
        ("1.0.2", "1.0.1", true),
        ("1.1.0", "1.0.9", true),
        ("2.0.0", "1.9.9", true),
        ("1.0.1", "1.0.1", false),
        ("1.0.0", "1.0.1", false),
        ("v1.0.1", "1.0.0", true),
        ("1.0.0", "dev", true),
        ("1.0.0", "0.0.0-dev.abc1234", true),
        ("1.0.1", "1.0.1-dev.x", false),
        // This Rust build: 4.0.0-dev counts as 4.0.0, so 3.5.0 isn't offered.
        ("3.5.0", "4.0.0-dev", false),
        ("4.0.1", "4.0.0-dev", true),
    ];
    for (a, b, want) in cases {
        assert_eq!(newer(a, b), want, "newer({a}, {b})");
    }
}

#[test]
fn test_versions_are_ordered() {
    let order = [
        "0.0.0-dev.abc",
        "3.0.9",
        "3.1.0-beta.1",
        "3.1.0-beta.2",
        "3.1.0-beta.10",
        "3.1.0",
        "v3.1.1",
    ];
    for (i, a) in order.iter().enumerate() {
        for (j, b) in order.iter().enumerate() {
            assert_eq!(compare(a, b), i.cmp(&j), "compare({a}, {b})");
        }
    }
}

type Routes = Arc<Mutex<HashMap<String, (u16, Vec<u8>)>>>;

/// A tiny HTTP/1.1 server: path → (status, body). Serves until the test ends.
struct Server {
    base: String,
    routes: Routes,
}

impl Server {
    fn start() -> Server {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", l.local_addr().unwrap());
        let routes: Routes = Arc::default();
        let r = routes.clone();
        std::thread::spawn(move || {
            for conn in l.incoming() {
                let Ok(mut conn) = conn else { continue };
                let mut reader = BufReader::new(conn.try_clone().unwrap());
                let mut line = String::new();
                if reader.read_line(&mut line).is_err() {
                    continue;
                }
                // Skip the headers.
                loop {
                    let mut h = String::new();
                    if reader.read_line(&mut h).is_err() || h.trim().is_empty() {
                        break;
                    }
                }
                let path = line.split_whitespace().nth(1).unwrap_or("/").to_string();
                let (status, body) = r
                    .lock()
                    .unwrap()
                    .get(&path)
                    .cloned()
                    .unwrap_or((404, b"not found".to_vec()));
                let _ = write!(
                    conn,
                    "HTTP/1.1 {status} X\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = conn.write_all(&body);
            }
        });
        Server { base, routes }
    }

    fn route(&self, path: &str, body: impl Into<Vec<u8>>) {
        self.routes
            .lock()
            .unwrap()
            .insert(path.to_string(), (200, body.into()));
    }

    fn updater(&self) -> Updater {
        let mut u = Updater::new(None);
        u.api_base = self.base.clone();
        u
    }
}

const PLIST: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleIdentifier</key><string>com.jamalxcode.jamalterm</string>
<key>CFBundleExecutable</key><string>jamalterm</string>
<key>CFBundlePackageType</key><string>APPL</string>
</dict></plist>"#;

/// Makes a jamalterm.app in `dir` whose executable holds `content`, and
/// returns the executable's final bytes. On a Mac it's a real ad-hoc signed
/// bundle, so the bundle id and codesign checks run like for a real update.
fn make_app(dir: &Path, content: &[u8]) -> (PathBuf, Vec<u8>) {
    let app = dir.join("jamalterm.app");
    std::fs::create_dir_all(app.join("Contents/MacOS")).unwrap();
    let exe = app.join("Contents/MacOS/jamalterm");
    std::fs::write(&exe, content).unwrap();
    std::fs::write(app.join("Contents/Info.plist"), PLIST).unwrap();
    #[cfg(target_os = "macos")]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755)).unwrap();
        let out = std::process::Command::new("/usr/bin/codesign")
            .args(["--force", "--deep", "--sign", "-"])
            .arg(&app)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "codesign: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    let final_bytes = std::fs::read(&exe).unwrap();
    (app, final_bytes)
}

/// A real program on a Mac (codesign needs one), plain text elsewhere.
fn program(mac: &str, other: &[u8]) -> Vec<u8> {
    if cfg!(target_os = "macos") {
        std::fs::read(mac).unwrap()
    } else {
        other.to_vec()
    }
}

/// Zips a jamalterm.app: returns the zip and the executable's bytes.
fn zip_app(content: &[u8]) -> (Vec<u8>, Vec<u8>) {
    let dir = tempfile::tempdir().unwrap();
    let (app, exe) = make_app(dir.path(), content);
    let zip_path = dir.path().join("app.zip");
    #[cfg(target_os = "macos")]
    {
        let ok = std::process::Command::new("/usr/bin/ditto")
            .args(["-c", "-k", "--keepParent"])
            .arg(&app)
            .arg(&zip_path)
            .status()
            .unwrap()
            .success();
        assert!(ok, "ditto");
    }
    #[cfg(not(target_os = "macos"))]
    {
        let mut w = zip::ZipWriter::new(std::fs::File::create(&zip_path).unwrap());
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .unix_permissions(0o755);
        for f in ["Contents/MacOS/jamalterm", "Contents/Info.plist"] {
            w.start_file(format!("jamalterm.app/{f}"), opts).unwrap();
            w.write_all(&std::fs::read(app.join(f)).unwrap()).unwrap();
        }
        w.finish().unwrap();
    }
    (std::fs::read(&zip_path).unwrap(), exe)
}

/// Serves a GitHub-like release 9.9.9 holding `content`. Returns the
/// executable's bytes and the zip's checksum. The real Sigstore check is
/// tested in provenance.rs; here the updater must only call it right.
fn fake_release(s: &Server, u: &mut Updater, content: &[u8], corrupt_sum: bool) -> Vec<u8> {
    let (zip, exe) = zip_app(content);
    let sum = provenance::to_hex(&Sha256::digest(&zip));
    let listed = if corrupt_sum {
        "0".repeat(64)
    } else {
        sum.clone()
    };
    s.route(
        "/repos/jamalxcode/jamalterm/releases/latest",
        format!(
            r#"{{"tag_name":"v9.9.9","body":"New stuff","html_url":"https://example/rel","assets":[
            {{"name":"jamalterm-9.9.9.dmg","browser_download_url":"{0}/x.dmg"}},
            {{"name":"jamalterm-9.9.9-macos.zip","browser_download_url":"{0}/app.zip"}},
            {{"name":"jamalterm-9.9.9-macos.zip.sigstore.json","browser_download_url":"{0}/proof"}},
            {{"name":"SHA256SUMS.txt","browser_download_url":"{0}/sums"}}]}}"#,
            s.base
        ),
    );
    s.route("/app.zip", zip);
    s.route("/proof", "PROOF");
    s.route(
        "/sums",
        format!(
            "{}  jamalterm-9.9.9.dmg\n{listed}  jamalterm-9.9.9-macos.zip\n",
            "0".repeat(64)
        ),
    );
    u.verify = Box::new(move |b, sha| {
        if b == b"PROOF" && sha == sum {
            Ok(())
        } else {
            Err(format!(
                "checked the wrong thing: {:?} {sha}",
                String::from_utf8_lossy(b)
            ))
        }
    });
    exe
}

fn installed_app() -> (tempfile::TempDir, PathBuf, Vec<u8>) {
    let dir = tempfile::tempdir().unwrap();
    let (app, exe) = make_app(dir.path(), &program("/bin/echo", b"old"));
    (dir, app, exe)
}

fn exe_of(app: &Path) -> Vec<u8> {
    std::fs::read(app.join("Contents/MacOS/jamalterm")).unwrap_or_default()
}

fn entries(dir: &Path) -> usize {
    std::fs::read_dir(dir).unwrap().count()
}

#[test]
fn install_replaces_the_app() {
    let s = Server::start();
    let mut u = s.updater();
    let content = fake_release(&s, &mut u, &program("/usr/bin/true", b"new version"), false);
    let rel = u.latest(Channel::Stable).unwrap();
    assert_eq!(
        (
            rel.version.as_str(),
            rel.notes.as_str(),
            rel.zip_name.as_str()
        ),
        ("9.9.9", "New stuff", "jamalterm-9.9.9-macos.zip")
    );
    let (dir, app, _) = installed_app();
    let mut calls = 0;
    u.install(&rel, &app, &mut |_, _| calls += 1).unwrap();
    assert_eq!(exe_of(&app), content);
    assert!(calls > 0, "progress was reported");
    // No staging folders are left next to the app.
    assert_eq!(entries(dir.path()), 1);
}

#[test]
fn check_says_whether_theres_something_newer() {
    let s = Server::start();
    let mut u = s.updater();
    fake_release(&s, &mut u, b"x", false);
    assert_eq!(u.check(Channel::Stable, "9.9.9").unwrap(), None);
    assert_eq!(
        u.check(Channel::Stable, "9.9.9-beta.3")
            .unwrap()
            .unwrap()
            .version,
        "9.9.9"
    );
    assert!(
        u.check(Channel::Stable, current_version())
            .unwrap()
            .is_some()
    );
}

#[test]
fn a_bad_checksum_changes_nothing() {
    let s = Server::start();
    let mut u = s.updater();
    fake_release(&s, &mut u, b"evil", true);
    let rel = u.latest(Channel::Stable).unwrap();
    let (dir, app, old) = installed_app();
    let err = u.install(&rel, &app, &mut |_, _| {}).unwrap_err();
    assert!(err.contains("checksum mismatch"), "{err}");
    assert_eq!(exe_of(&app), old);
    assert_eq!(entries(dir.path()), 1);
}

#[test]
fn a_failed_proof_changes_nothing() {
    let s = Server::start();
    let mut u = s.updater();
    fake_release(&s, &mut u, b"new", false);
    let rel = u.latest(Channel::Stable).unwrap();
    // The real check: "PROOF" isn't a Sigstore bundle.
    u.verify = Box::new(provenance::verify);
    let (_dir, app, old) = installed_app();
    let err = u.install(&rel, &app, &mut |_, _| {}).unwrap_err();
    assert!(err.starts_with("the build proof is damaged"), "{err}");
    assert!(err.ends_with("; nothing was changed"), "{err}");
    assert_eq!(exe_of(&app), old);
}

#[test]
fn a_release_without_proof_is_refused() {
    let s = Server::start();
    let mut u = s.updater();
    fake_release(&s, &mut u, b"new", false);
    let mut rel = u.latest(Channel::Stable).unwrap();
    rel.bundle_url = None;
    let (_dir, app, old) = installed_app();
    let err = u.install(&rel, &app, &mut |_, _| {}).unwrap_err();
    assert!(err.contains("no proof"), "{err}");
    assert_eq!(exe_of(&app), old);
}

#[test]
fn a_zip_without_the_app_is_refused() {
    let s = Server::start();
    let mut u = s.updater();
    fake_release(&s, &mut u, b"new", false);
    let rel = u.latest(Channel::Stable).unwrap();
    // Replace the zip (and its checksum) with one holding something else.
    #[cfg(not(target_os = "macos"))]
    {
        let mut buf = io::Cursor::new(Vec::new());
        let mut w = zip::ZipWriter::new(&mut buf);
        w.start_file("other.app/readme", zip::write::SimpleFileOptions::default())
            .unwrap();
        w.write_all(b"hi").unwrap();
        w.finish().unwrap();
        let zip = buf.into_inner();
        let sum = provenance::to_hex(&Sha256::digest(&zip));
        s.route("/app.zip", zip);
        s.route("/sums", format!("{sum}  jamalterm-9.9.9-macos.zip\n"));
        u.verify = Box::new(|_, _| Ok(()));
        let (_dir, app, old) = installed_app();
        let err = u.install(&rel, &app, &mut |_, _| {}).unwrap_err();
        assert_eq!(err, "the update doesn't contain jamalterm.app");
        assert_eq!(exe_of(&app), old);
    }
    let _ = rel;
}

#[test]
fn keep_and_revert() {
    let s = Server::start();
    let mut u = s.updater();
    let content = fake_release(&s, &mut u, &program("/usr/bin/true", b"new version"), false);
    let keep = tempfile::tempdir().unwrap();
    u.keep_previous_in = Some(keep.path().join("previous"));
    let rel = u.latest(Channel::Stable).unwrap();
    let (dir, app, old) = installed_app();
    assert_eq!(u.previous_app(), None, "nothing kept yet");
    assert!(u.revert(&app).unwrap_err().contains("no earlier version"));

    u.install(&rel, &app, &mut |_, _| {}).unwrap();
    let prev = u.previous_app().expect("old version kept");
    assert_eq!(exe_of(&prev), old);

    // Going back: the old one is in place, the new one kept.
    u.revert(&app).unwrap();
    assert_eq!(exe_of(&app), old);
    assert_eq!(exe_of(&u.previous_app().unwrap()), content);
    assert_eq!(entries(dir.path()), 1, "leftovers next to the app");

    // And forward again.
    u.revert(&app).unwrap();
    assert_eq!(exe_of(&app), content);
}

#[test]
fn the_beta_channel_takes_prereleases_but_never_drafts() {
    let s = Server::start();
    let asset = |v: &str| {
        format!(
            r#"[{{"name":"jamalterm-{v}-macos.zip","browser_download_url":"z"}},{{"name":"SHA256SUMS.txt","browser_download_url":"s"}}]"#
        )
    };
    s.route(
        "/repos/jamalxcode/jamalterm/releases/latest",
        format!(r#"{{"tag_name":"v3.0.0","assets":{}}}"#, asset("3.0.0")),
    );
    s.route(
        "/repos/jamalxcode/jamalterm/releases?per_page=20",
        format!(
            r#"[
            {{"tag_name":"v3.1.0-beta.2","prerelease":true,"assets":{}}},
            {{"tag_name":"v3.2.0-beta.1","draft":true,"assets":{}}},
            {{"tag_name":"v3.1.0-beta.1","prerelease":true,"assets":{}}},
            {{"tag_name":"v3.0.0","body":null,"assets":{}}}]"#,
            asset("3.1.0-beta.2"),
            asset("3.2.0-beta.1"),
            asset("3.1.0-beta.1"),
            asset("3.0.0")
        ),
    );
    let u = s.updater();
    assert_eq!(u.latest(Channel::Stable).unwrap().version, "3.0.0");
    let beta = u.latest(Channel::Beta).unwrap();
    assert_eq!(beta.version, "3.1.0-beta.2");
    // A Beta release without a proof is found, but refused at install.
    assert_eq!(beta.bundle_url, None);
}

#[test]
fn github_errors_are_plain_words() {
    let s = Server::start();
    let u = s.updater();
    assert_eq!(
        u.latest(Channel::Stable).unwrap_err(),
        "GitHub answered 404"
    );
    s.route("/repos/jamalxcode/jamalterm/releases?per_page=20", "[]");
    assert_eq!(u.latest(Channel::Beta).unwrap_err(), "no releases yet");
    s.route(
        "/repos/jamalxcode/jamalterm/releases/latest",
        r#"{"tag_name":"v9.0.0","assets":[]}"#,
    );
    assert_eq!(
        u.latest(Channel::Stable).unwrap_err(),
        "the latest release has no macOS download yet"
    );
    let mut off = Updater::new(None);
    off.api_base = "http://127.0.0.1:1".into();
    assert!(
        off.latest(Channel::Stable)
            .unwrap_err()
            .starts_with("couldn't reach GitHub: ")
    );
}

#[test]
fn checksums_file() {
    let a = "a".repeat(64);
    let b = "b".repeat(64);
    let sums = format!("{a}  jamalterm-1.dmg\n{b} *jamalterm-1-macos.zip\n");
    assert_eq!(sum_for(&sums, "jamalterm-1-macos.zip"), Some(b));
    assert_eq!(sum_for(&sums, "jamalterm-2-macos.zip"), None);
    assert_eq!(
        sum_for("abc  jamalterm-1-macos.zip", "jamalterm-1-macos.zip"),
        None
    );
}

#[test]
fn finds_the_app_bundle() {
    assert_eq!(
        app_bundle_of(Path::new(
            "/Applications/jamalterm.app/Contents/MacOS/jamalterm"
        ))
        .unwrap(),
        PathBuf::from("/Applications/jamalterm.app")
    );
    assert!(app_bundle_of(Path::new("/home/me/target/debug/jamalterm")).is_err());
}

#[test]
fn status_text() {
    let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000);
    let mut st = Status::new(Channel::Stable);
    st.version = "4.0.0".into();
    assert_eq!(st.text(now), "jamalterm 4.0.0. Not checked yet.");
    st.checked(Ok(None), now - Duration::from_secs(5 * 60));
    assert_eq!(
        st.text(now),
        "jamalterm 4.0.0. Last checked 5 minutes ago. You're up to date."
    );
    st.channel = Channel::Beta;
    st.checked(Err("couldn't reach GitHub: offline".into()), now);
    assert_eq!(
        st.text(now),
        "jamalterm 4.0.0 (Beta channel). Last checked just now. ✕ Couldn't reach GitHub: offline"
    );
    st.state = State::Downloading {
        done: 50,
        total: Some(200),
    };
    assert!(st.text(now).ends_with("Downloading… 25%"));
    st.last_checked = Some(now - Duration::from_secs(3600));
    st.state = State::Installed {
        version: "4.0.1".into(),
    };
    assert!(
        st.text(now)
            .contains("Last checked 1 hour ago. Version 4.0.1 is installed; restart")
    );
}
