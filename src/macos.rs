//! Small macOS pieces: the desktop picture, Reduce Motion, notifications,
//! a Dock bounce and the finish-line chime. Elsewhere they do nothing.

/// The desktop picture as a few thousand pixels (for Match my wallpaper),
/// shrunk with the built-in `sips` (it reads HEIC, JPEG and PNG).
pub fn wallpaper_pixels() -> Option<Vec<[u8; 3]>> {
    if !cfg!(target_os = "macos") {
        return None;
    }
    let path = wallpaper_path()?;
    let out = std::env::temp_dir().join(format!("jamalterm-wallpaper-{}.png", std::process::id()));
    let ok = std::process::Command::new("/usr/bin/sips")
        .args(["-s", "format", "png", "-Z", "64"])
        .arg(&path)
        .arg("--out")
        .arg(&out)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    let px = ok.then(|| read_png(&out)).flatten();
    let _ = std::fs::remove_file(&out);
    px
}

fn read_png(path: &std::path::Path) -> Option<Vec<[u8; 3]>> {
    let mut dec = png::Decoder::new(std::io::BufReader::new(std::fs::File::open(path).ok()?));
    dec.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut r = dec.read_info().ok()?;
    let mut buf = vec![0; r.output_buffer_size()];
    let info = r.next_frame(&mut buf).ok()?;
    let step = match info.color_type {
        png::ColorType::Rgba => 4,
        png::ColorType::Rgb => 3,
        _ => return None,
    };
    Some(
        buf[..info.buffer_size()]
            .chunks(step)
            .map(|c| [c[0], c[1], c[2]])
            .collect(),
    )
}

#[cfg(target_os = "macos")]
fn wallpaper_path() -> Option<std::path::PathBuf> {
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSScreen, NSWorkspace};
    let mtm = MainThreadMarker::new()?;
    let screen = NSScreen::mainScreen(mtm)?;
    let url = NSWorkspace::sharedWorkspace().desktopImageURLForScreen(&screen)?;
    url.path().map(|p| p.to_string().into())
}

#[cfg(not(target_os = "macos"))]
fn wallpaper_path() -> Option<std::path::PathBuf> {
    None
}

/// System Settings → Accessibility → Reduce motion.
pub fn reduce_motion() -> bool {
    #[cfg(target_os = "macos")]
    {
        objc2_app_kit::NSWorkspace::sharedWorkspace().accessibilityDisplayShouldReduceMotion()
    }
    #[cfg(not(target_os = "macos"))]
    false
}

/// A notification (when a long command ends while you're in another app).
/// macOS asks once whether jamalterm may notify you; if it may not, the
/// Dock icon bounces instead.
pub fn notify(title: &str, body: &str) {
    #[cfg(target_os = "macos")]
    {
        use objc2_foundation::{NSBundle, NSString};
        use objc2_user_notifications::{
            UNAuthorizationOptions, UNMutableNotificationContent, UNNotificationRequest,
            UNUserNotificationCenter,
        };
        // Notifications need an app bundle (not `cargo run`).
        if NSBundle::mainBundle().bundleIdentifier().is_none() {
            bounce();
            return;
        }
        let center = UNUserNotificationCenter::currentNotificationCenter();
        let opts = UNAuthorizationOptions::Alert | UNAuthorizationOptions::Sound;
        let (t, b) = (title.to_string(), body.to_string());
        let done = block2::RcBlock::new(
            move |granted: objc2::runtime::Bool, _err: *mut objc2_foundation::NSError| {
                if !granted.as_bool() {
                    return; // the bounce below already told
                }
                let content = UNMutableNotificationContent::new();
                content.setTitle(&NSString::from_str(&t));
                content.setBody(&NSString::from_str(&b));
                let id = NSString::from_str(&format!("jamalterm-{}", crate::fun::now_secs()));
                let req = UNNotificationRequest::requestWithIdentifier_content_trigger(
                    &id, &content, None,
                );
                UNUserNotificationCenter::currentNotificationCenter()
                    .addNotificationRequest_withCompletionHandler(&req, None);
            },
        );
        center.requestAuthorizationWithOptions_completionHandler(opts, &done);
        bounce();
    }
    #[cfg(not(target_os = "macos"))]
    let _ = (title, body);
}

/// Bounces the Dock icon once.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn bounce() {
    #[cfg(target_os = "macos")]
    if let Some(mtm) = objc2::MainThreadMarker::new() {
        use objc2_app_kit::{NSApplication, NSRequestUserAttentionType};
        NSApplication::sharedApplication(mtm)
            .requestUserAttention(NSRequestUserAttentionType::InformationalRequest);
    }
}

/// Brings jamalterm to the front, as choosing one of its windows from the
/// Dock or the menu bar icon should: shown again if hidden (⌘H), then made
/// the active app.
pub fn activate() {
    #[cfg(target_os = "macos")]
    if let Some(mtm) = objc2::MainThreadMarker::new() {
        use objc2::runtime::NSObjectProtocol;
        use objc2_app_kit::NSApplication;
        let app = NSApplication::sharedApplication(mtm);
        app.unhide(None);
        // `activate` (macOS 14 and later) is the one that works there; the
        // older call before that.
        if app.respondsToSelector(objc2::sel!(activate)) {
            app.activate();
        } else {
            #[allow(deprecated)]
            app.activateIgnoringOtherApps(true);
        }
    }
}

/// The finish line's soft chime (off by default).
pub fn chime() {
    #[cfg(target_os = "macos")]
    {
        use objc2_app_kit::NSSound;
        use objc2_foundation::NSString;
        if let Some(s) = NSSound::soundNamed(&NSString::from_str("Glass")) {
            s.play();
        }
    }
}

/// Quick Look (Space in browse mode on a path).
pub fn quick_look(path: &std::path::Path) {
    if cfg!(target_os = "macos") {
        let _ = std::process::Command::new("/usr/bin/qlmanage")
            .arg("-p")
            .arg(path)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
    }
}

/// Opens a file, folder or web page with its usual app (⌘-click).
pub fn open(target: &str) {
    let opener = if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    let _ = std::process::Command::new(opener).arg(target).spawn();
}
