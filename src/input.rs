//! Keys to the bytes a terminal sends. Typed text comes separately (egui's
//! Text events, which also carry input methods); this handles the keys that
//! aren't text: arrows, Return, Ctrl combinations, Option as Meta…

use egui::{Key, Modifiers};

/// Is this the app's own shortcut modifier? ⌘ on a Mac; Ctrl+Shift on Linux
/// development builds (so the shell keeps Ctrl+W and friends).
pub fn is_app_shortcut(m: &Modifiers) -> bool {
    if cfg!(target_os = "macos") {
        m.mac_cmd
    } else {
        m.ctrl && m.shift
    }
}

/// The xterm modifier number for CSI sequences (1 = none).
fn xterm_mod(m: &Modifiers) -> u8 {
    1 + (m.shift as u8) + 2 * (m.alt as u8) + 4 * (m.ctrl as u8)
}

/// Bytes for a key press, or None when it's plain text (sent from the Text
/// event instead) or not ours. `app_cursor`: the program asked for
/// "application cursor keys" (vim, less), so arrows send ESC O A.
pub fn key_bytes(key: Key, m: &Modifiers, app_cursor: bool, option_meta: bool) -> Option<Vec<u8>> {
    let modn = xterm_mod(m);
    let csi = |c: char| -> Vec<u8> {
        if modn > 1 {
            format!("\x1b[1;{modn}{c}").into_bytes()
        } else if app_cursor {
            format!("\x1bO{c}").into_bytes()
        } else {
            format!("\x1b[{c}").into_bytes()
        }
    };
    let tilde = |n: u8| -> Vec<u8> {
        if modn > 1 {
            format!("\x1b[{n};{modn}~").into_bytes()
        } else {
            format!("\x1b[{n}~").into_bytes()
        }
    };
    let bytes = match key {
        Key::ArrowUp => csi('A'),
        Key::ArrowDown => csi('B'),
        Key::ArrowRight => csi('C'),
        Key::ArrowLeft => csi('D'),
        Key::Home => csi('H'),
        Key::End => csi('F'),
        Key::PageUp => tilde(5),
        Key::PageDown => tilde(6),
        Key::Insert => tilde(2),
        Key::Delete => tilde(3),
        Key::Enter => {
            if m.alt && option_meta {
                b"\x1b\r".to_vec()
            } else {
                b"\r".to_vec()
            }
        }
        Key::Tab => {
            if m.shift {
                b"\x1b[Z".to_vec()
            } else {
                b"\t".to_vec()
            }
        }
        Key::Backspace => {
            if m.alt && option_meta {
                b"\x1b\x7f".to_vec() // delete a word
            } else if m.ctrl {
                b"\x08".to_vec()
            } else {
                b"\x7f".to_vec()
            }
        }
        Key::Escape => b"\x1b".to_vec(),
        Key::F1 => b"\x1bOP".to_vec(),
        Key::F2 => b"\x1bOQ".to_vec(),
        Key::F3 => b"\x1bOR".to_vec(),
        Key::F4 => b"\x1bOS".to_vec(),
        Key::F5 => tilde(15),
        Key::F6 => tilde(17),
        Key::F7 => tilde(18),
        Key::F8 => tilde(19),
        Key::F9 => tilde(20),
        Key::F10 => tilde(21),
        Key::F11 => tilde(23),
        Key::F12 => tilde(24),
        _ => {
            // Ctrl+letter → control character; Option+key as Meta → ESC + key.
            let name = key.name();
            let ch = name.chars().next().filter(|_| name.chars().count() == 1)?;
            if m.ctrl && !m.mac_cmd {
                let c = ch.to_ascii_lowercase();
                return match c {
                    'a'..='z' => Some(vec![c as u8 - b'a' + 1]),
                    '[' => Some(vec![0x1b]),
                    '\\' => Some(vec![0x1c]),
                    ']' => Some(vec![0x1d]),
                    ' ' | '2' | '@' => Some(vec![0]),
                    _ => None,
                };
            }
            if m.alt && option_meta && !m.mac_cmd && ch.is_ascii_alphanumeric() {
                let c = if m.shift {
                    ch.to_ascii_uppercase()
                } else {
                    ch.to_ascii_lowercase()
                };
                return Some(vec![0x1b, c as u8]);
            }
            return None;
        }
    };
    Some(bytes)
}

/// Pasted text as the shell should get it: line breaks as Return, wrapped in
/// bracketed-paste markers when the shell asked for them (so a pasted command
/// isn't run line by line).
pub fn paste_bytes(text: &str, bracketed: bool) -> Vec<u8> {
    let body = text.replace("\r\n", "\r").replace('\n', "\r");
    if bracketed {
        // A pasted end marker must not end the paste early.
        let body = body.replace("\x1b[201~", "");
        format!("\x1b[200~{body}\x1b[201~").into_bytes()
    } else {
        body.into_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn none() -> Modifiers {
        Modifiers::NONE
    }

    #[test]
    fn arrows_and_cursor_mode() {
        assert_eq!(
            key_bytes(Key::ArrowUp, &none(), false, true).unwrap(),
            b"\x1b[A"
        );
        assert_eq!(
            key_bytes(Key::ArrowUp, &none(), true, true).unwrap(),
            b"\x1bOA",
            "vim and less ask for this"
        );
        let shift = Modifiers {
            shift: true,
            ..none()
        };
        assert_eq!(
            key_bytes(Key::ArrowLeft, &shift, false, true).unwrap(),
            b"\x1b[1;2D"
        );
        assert_eq!(
            key_bytes(Key::Delete, &none(), false, true).unwrap(),
            b"\x1b[3~"
        );
    }

    #[test]
    fn control_and_meta() {
        let ctrl = Modifiers {
            ctrl: true,
            ..none()
        };
        assert_eq!(key_bytes(Key::C, &ctrl, false, true).unwrap(), vec![3]);
        assert_eq!(key_bytes(Key::D, &ctrl, false, true).unwrap(), vec![4]);
        let alt = Modifiers {
            alt: true,
            ..none()
        };
        assert_eq!(
            key_bytes(Key::B, &alt, false, true).unwrap(),
            b"\x1bb",
            "⌥B: back a word"
        );
        assert_eq!(
            key_bytes(Key::B, &alt, false, false),
            None,
            "Option as Meta off: the text event types ∫"
        );
        assert_eq!(
            key_bytes(Key::A, &none(), false, true),
            None,
            "plain letters come as text"
        );
    }

    #[test]
    fn paste() {
        assert_eq!(paste_bytes("a\nb", false), b"a\rb");
        assert_eq!(paste_bytes("ls\n", true), b"\x1b[200~ls\r\x1b[201~");
        assert_eq!(
            paste_bytes("x\x1b[201~rm -rf", true),
            b"\x1b[200~xrm -rf\x1b[201~",
            "can't break out of the paste"
        );
    }
}
