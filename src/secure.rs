//! Secure Keyboard Entry (macOS): stops other apps from reading what you
//! type. It turns on by itself while the pane in front is at a password
//! prompt (the shell's terminal has echo off in line mode, as sudo and ssh
//! set it), and only while jamalterm is the active app; the jamalterm menu
//! can keep it on all the time. A 🔒 shows while it's on.

use std::sync::atomic::{AtomicBool, Ordering};

static ON: AtomicBool = AtomicBool::new(false);

#[cfg(target_os = "macos")]
#[link(name = "Carbon", kind = "framework")]
unsafe extern "C" {
    fn EnableSecureEventInput() -> i32;
    fn DisableSecureEventInput() -> i32;
}

/// Turns it on or off. macOS counts the calls, so they're kept balanced.
pub fn set(want: bool) {
    let changed = ON.swap(want, Ordering::SeqCst) != want;
    #[cfg(target_os = "macos")]
    if changed {
        unsafe {
            if want {
                EnableSecureEventInput();
            } else {
                DisableSecureEventInput();
            }
        }
    }
    let _ = changed;
}

pub fn on() -> bool {
    ON.load(Ordering::SeqCst)
}

/// A terminal at a password prompt: echo off, in line (canonical) mode.
pub fn password_prompt(fd: Option<i32>) -> bool {
    let Some(fd) = fd else {
        return false;
    };
    let mut t: libc::termios = unsafe { std::mem::zeroed() };
    if unsafe { libc::tcgetattr(fd, &mut t) } != 0 {
        return false;
    }
    t.c_lflag & libc::ECHO == 0 && t.c_lflag & libc::ICANON != 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn balanced() {
        set(true);
        set(true);
        assert!(on());
        set(false);
        set(false);
        assert!(!on());
    }

    #[test]
    fn spots_a_password_prompt() {
        // A real pty: echo on at first, then off as `read -s` sets it.
        use portable_pty::{CommandBuilder, PtySize, native_pty_system};
        let pty = native_pty_system()
            .openpty(PtySize {
                rows: 24,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            })
            .unwrap();
        let fd = pty.master.as_raw_fd();
        assert!(!password_prompt(fd), "a fresh terminal echoes");
        let mut cmd = CommandBuilder::new("/bin/sh");
        cmd.args(["-c", "stty -echo; sleep 2"]);
        let mut child = pty.slave.spawn_command(cmd).unwrap();
        let until = std::time::Instant::now() + std::time::Duration::from_secs(2);
        let mut seen = false;
        while std::time::Instant::now() < until && !seen {
            seen = password_prompt(fd);
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        let _ = child.kill();
        assert!(seen, "echo off in line mode: a password prompt");
        assert!(!password_prompt(None));
    }
}
