//! The built-in themes, the same colors as jamalterm 3.x.
//! Generated from the old frontend/src/themes.ts; edit here now.
use egui::Color32;
#[derive(Debug, Clone)]
#[allow(dead_code)] // name and light: for Settings (next phase)
pub struct Theme {
    pub id: &'static str,
    pub name: &'static str,
    pub light: bool,
    pub background: Color32,
    pub foreground: Color32,
    pub cursor: Color32,
    pub selection: Color32,
    /// The 16 ANSI colors: black, red, green, yellow, blue, magenta, cyan,
    /// white, then the bright ones.
    pub ansi: [Color32; 16],
}

/// "#rrggbb" or "#rrggbbaa".
pub fn hex(s: &str) -> Color32 {
    let b = s.as_bytes();
    fn h(c: u8) -> u8 {
        match c {
            b'0'..=b'9' => c - b'0',
            b'a'..=b'f' => c - b'a' + 10,
            b'A'..=b'F' => c - b'A' + 10,
            _ => 0,
        }
    }
    let byte = |i: usize| h(b[i]) * 16 + h(b[i + 1]);
    if b.len() >= 9 {
        Color32::from_rgba_unmultiplied(byte(1), byte(3), byte(5), byte(7))
    } else {
        Color32::from_rgb(byte(1), byte(3), byte(5))
    }
}

pub fn themes() -> Vec<Theme> {
    vec![
        Theme {
            id: "jamal-night",
            name: "Jamal Night",
            light: false,
            background: hex("#15161e"),
            foreground: hex("#c8d3f5"),
            cursor: hex("#ff8fb1"),
            selection: hex("#3d59a1aa"),
            ansi: [
                hex("#1b1d2b"),
                hex("#ff757f"),
                hex("#c3e88d"),
                hex("#ffc777"),
                hex("#82aaff"),
                hex("#c099ff"),
                hex("#86e1fc"),
                hex("#a9b8e8"),
                hex("#545c7e"),
                hex("#ff8d94"),
                hex("#c7fb6d"),
                hex("#ffd8ab"),
                hex("#9ab8ff"),
                hex("#caabff"),
                hex("#b2ebff"),
                hex("#d0daff"),
            ],
        },
        Theme {
            id: "catppuccin-mocha",
            name: "Catppuccin Mocha",
            light: false,
            background: hex("#1e1e2e"),
            foreground: hex("#cdd6f4"),
            cursor: hex("#f5e0dc"),
            selection: hex("#585b7099"),
            ansi: [
                hex("#45475a"),
                hex("#f38ba8"),
                hex("#a6e3a1"),
                hex("#f9e2af"),
                hex("#89b4fa"),
                hex("#f5c2e7"),
                hex("#94e2d5"),
                hex("#bac2de"),
                hex("#585b70"),
                hex("#f38ba8"),
                hex("#a6e3a1"),
                hex("#f9e2af"),
                hex("#89b4fa"),
                hex("#f5c2e7"),
                hex("#94e2d5"),
                hex("#a6adc8"),
            ],
        },
        Theme {
            id: "dracula",
            name: "Dracula",
            light: false,
            background: hex("#282a36"),
            foreground: hex("#f8f8f2"),
            cursor: hex("#f8f8f2"),
            selection: hex("#44475a"),
            ansi: [
                hex("#21222c"),
                hex("#ff5555"),
                hex("#50fa7b"),
                hex("#f1fa8c"),
                hex("#bd93f9"),
                hex("#ff79c6"),
                hex("#8be9fd"),
                hex("#f8f8f2"),
                hex("#6272a4"),
                hex("#ff6e6e"),
                hex("#69ff94"),
                hex("#ffffa5"),
                hex("#d6acff"),
                hex("#ff92df"),
                hex("#a4ffff"),
                hex("#ffffff"),
            ],
        },
        Theme {
            id: "tokyo-night",
            name: "Tokyo Night",
            light: false,
            background: hex("#1a1b26"),
            foreground: hex("#c0caf5"),
            cursor: hex("#c0caf5"),
            selection: hex("#33467c"),
            ansi: [
                hex("#15161e"),
                hex("#f7768e"),
                hex("#9ece6a"),
                hex("#e0af68"),
                hex("#7aa2f7"),
                hex("#bb9af7"),
                hex("#7dcfff"),
                hex("#a9b1d6"),
                hex("#414868"),
                hex("#f7768e"),
                hex("#9ece6a"),
                hex("#e0af68"),
                hex("#7aa2f7"),
                hex("#bb9af7"),
                hex("#7dcfff"),
                hex("#c0caf5"),
            ],
        },
        Theme {
            id: "one-dark",
            name: "One Dark",
            light: false,
            background: hex("#282c34"),
            foreground: hex("#abb2bf"),
            cursor: hex("#528bff"),
            selection: hex("#3e4451"),
            ansi: [
                hex("#282c34"),
                hex("#e06c75"),
                hex("#98c379"),
                hex("#e5c07b"),
                hex("#61afef"),
                hex("#c678dd"),
                hex("#56b6c2"),
                hex("#abb2bf"),
                hex("#5c6370"),
                hex("#e06c75"),
                hex("#98c379"),
                hex("#e5c07b"),
                hex("#61afef"),
                hex("#c678dd"),
                hex("#56b6c2"),
                hex("#ffffff"),
            ],
        },
        Theme {
            id: "nord",
            name: "Nord",
            light: false,
            background: hex("#2e3440"),
            foreground: hex("#d8dee9"),
            cursor: hex("#d8dee9"),
            selection: hex("#434c5e"),
            ansi: [
                hex("#3b4252"),
                hex("#bf616a"),
                hex("#a3be8c"),
                hex("#ebcb8b"),
                hex("#81a1c1"),
                hex("#b48ead"),
                hex("#88c0d0"),
                hex("#e5e9f0"),
                hex("#4c566a"),
                hex("#bf616a"),
                hex("#a3be8c"),
                hex("#ebcb8b"),
                hex("#81a1c1"),
                hex("#b48ead"),
                hex("#8fbcbb"),
                hex("#eceff4"),
            ],
        },
        Theme {
            id: "gruvbox-dark",
            name: "Gruvbox Dark",
            light: false,
            background: hex("#282828"),
            foreground: hex("#ebdbb2"),
            cursor: hex("#ebdbb2"),
            selection: hex("#504945"),
            ansi: [
                hex("#282828"),
                hex("#d23d37"),
                hex("#98971a"),
                hex("#d79921"),
                hex("#458588"),
                hex("#b16286"),
                hex("#689d6a"),
                hex("#a89984"),
                hex("#928374"),
                hex("#fb4934"),
                hex("#b8bb26"),
                hex("#fabd2f"),
                hex("#83a598"),
                hex("#d3869b"),
                hex("#8ec07c"),
                hex("#ebdbb2"),
            ],
        },
        Theme {
            id: "solarized-dark",
            name: "Solarized Dark",
            light: false,
            background: hex("#002b36"),
            foreground: hex("#93a1a1"),
            cursor: hex("#93a1a1"),
            selection: hex("#073642"),
            ansi: [
                hex("#073642"),
                hex("#dc322f"),
                hex("#859900"),
                hex("#b58900"),
                hex("#268bd2"),
                hex("#d33682"),
                hex("#2aa198"),
                hex("#eee8d5"),
                hex("#586e75"),
                hex("#cb4b16"),
                hex("#859900"),
                hex("#b58900"),
                hex("#268bd2"),
                hex("#6c71c4"),
                hex("#2aa198"),
                hex("#fdf6e3"),
            ],
        },
        Theme {
            id: "github-light",
            name: "GitHub Light",
            light: true,
            background: hex("#ffffff"),
            foreground: hex("#1f2328"),
            cursor: hex("#0969da"),
            selection: hex("#0969da33"),
            ansi: [
                hex("#24292f"),
                hex("#cf222e"),
                hex("#116329"),
                hex("#4d2d00"),
                hex("#0969da"),
                hex("#8250df"),
                hex("#1b7c83"),
                hex("#6e7781"),
                hex("#57606a"),
                hex("#a40e26"),
                hex("#1a7f37"),
                hex("#633c01"),
                hex("#218bff"),
                hex("#a475f9"),
                hex("#3192aa"),
                hex("#8c959f"),
            ],
        },
        Theme {
            id: "solarized-light",
            name: "Solarized Light",
            light: true,
            background: hex("#fdf6e3"),
            foreground: hex("#586e75"),
            cursor: hex("#586e75"),
            selection: hex("#eee8d5"),
            ansi: [
                hex("#073642"),
                hex("#dc322f"),
                hex("#819400"),
                hex("#b08500"),
                hex("#268bd2"),
                hex("#d33682"),
                hex("#299c93"),
                hex("#eee8d5"),
                hex("#657b83"),
                hex("#cb4b16"),
                hex("#819400"),
                hex("#b08500"),
                hex("#268bd2"),
                hex("#6c71c4"),
                hex("#299c93"),
                hex("#002b36"),
            ],
        },
    ]
}

/// A theme by id; unknown ids give the first one. The specials:
/// "weekly" (Theme of the week) rotates through the built-in themes, and
/// "wallpaper" (Match my wallpaper) is made from the desktop picture.
pub fn by_id(id: &str) -> Theme {
    let all = themes();
    match id {
        "weekly" => return crate::fun::theme_of_the_week(&all),
        "wallpaper" => {
            if let Some(t) = crate::fun::wallpaper_theme() {
                return t;
            }
        }
        _ => {}
    }
    all.iter()
        .find(|t| t.id == id)
        .cloned()
        .unwrap_or_else(|| all[0].clone())
}

/// WCAG contrast ratio between two colors.
pub fn contrast(a: Color32, b: Color32) -> f32 {
    fn lum(c: Color32) -> f32 {
        let f = |v: u8| {
            let s = v as f32 / 255.0;
            if s <= 0.03928 {
                s / 12.92
            } else {
                ((s + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * f(c.r()) + 0.7152 * f(c.g()) + 0.0722 * f(c.b())
    }
    let (x, y) = (lum(a), lum(b));
    let (hi, lo) = if x > y { (x, y) } else { (y, x) };
    (hi + 0.05) / (lo + 0.05)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every theme stays readable (as in 3.x's contrast test): text 4.5:1,
    /// the 12 colors programs use 3:1.
    #[test]
    fn every_theme_is_readable() {
        for t in themes() {
            assert!(
                contrast(t.foreground, t.background) >= 4.5,
                "{} text",
                t.name
            );
            for (i, c) in t.ansi.iter().enumerate() {
                if matches!(i, 0 | 7 | 8 | 15) {
                    continue; // black/white pairs aren't text colors on their own background
                }
                assert!(contrast(*c, t.background) >= 3.0, "{} ansi {i}", t.name);
            }
        }
    }

    #[test]
    fn ten_themes_and_a_fallback() {
        assert_eq!(themes().len(), 10);
        assert_eq!(by_id("nord").name, "Nord");
        assert_eq!(by_id("nope").id, "jamal-night");
    }
}
