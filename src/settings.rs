//! settings.json: the same file and fields as jamalterm 3.x (Go), so moving to
//! the Rust app keeps everyone's settings. Fields this version doesn't use yet
//! are still read and written back unchanged.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostColor {
    pub pattern: String,
    pub color: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub theme: String,
    pub font_family: String,
    pub font_size: u32,
    pub line_height: f32,
    pub cursor_style: String, // block, bar, underline
    pub opacity: u32,
    pub window_frame: String,
    pub show_tab_bar: bool,
    pub blur: bool,
    pub cursor_blink: bool,
    pub ligatures: bool,
    pub project_colors: bool,
    pub finish_moments: bool,
    pub finish_sound: bool,
    pub menu_bar_icon: bool,
    pub cursor_effect: String,
    pub host_colors: Vec<HostColor>,
    pub scrollback: usize,
    pub shell: String, // empty = your login shell
    pub shell_integration: bool,
    pub arrow_browse: bool,
    pub restore_session: bool,
    pub notify_long_commands: bool,
    pub notify_after_seconds: u32,
    pub screen_reader: bool,
    pub prompt_keys: String,
    pub confirm_rerun: bool,
    pub option_is_meta: bool,
    pub secure_input: bool,
    pub keep_problem_log: bool,
    pub auto_update: bool,
    pub update_channel: String,
    pub ai_enabled: bool,
    pub ai_provider: String,
    pub ai_model: String,
    pub openai_model: String,
    pub deepseek_model: String,
    pub xai_model: String,
    pub openrouter_model: String,
    pub ai_share_output: bool,
    pub local_url: String,
    pub local_model: String,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            theme: "jamal-night".into(),
            font_family: r#""JetBrains Mono", monospace"#.into(),
            font_size: 14,
            line_height: 1.15,
            cursor_style: "block".into(),
            opacity: 100,
            window_frame: "system".into(),
            show_tab_bar: false,
            blur: true,
            cursor_blink: true,
            ligatures: false,
            project_colors: true,
            finish_moments: true,
            finish_sound: false,
            menu_bar_icon: true,
            cursor_effect: "none".into(),
            host_colors: vec![],
            scrollback: 10_000,
            shell: String::new(),
            shell_integration: true,
            arrow_browse: true,
            restore_session: true,
            notify_long_commands: true,
            notify_after_seconds: 10,
            screen_reader: false,
            prompt_keys: "emacs".into(),
            confirm_rerun: false,
            option_is_meta: true,
            secure_input: false,
            keep_problem_log: true,
            auto_update: true,
            update_channel: "stable".into(),
            ai_enabled: true,
            ai_provider: "anthropic".into(),
            ai_model: "claude-opus-5-5".into(),
            openai_model: String::new(),
            deepseek_model: String::new(),
            xai_model: String::new(),
            openrouter_model: String::new(),
            ai_share_output: true,
            local_url: "http://localhost:11434/v1".into(),
            local_model: String::new(),
        }
    }
}

impl Settings {
    /// Out-of-range values go back to their defaults (as the Go version did).
    pub fn normalized(mut self) -> Self {
        let d = Settings::default();
        if !(8..=48).contains(&self.font_size) {
            self.font_size = d.font_size;
        }
        if !(1.0..=2.0).contains(&self.line_height) {
            self.line_height = d.line_height;
        }
        if !matches!(self.cursor_style.as_str(), "block" | "bar" | "underline") {
            self.cursor_style = d.cursor_style;
        }
        if !(30..=100).contains(&self.opacity) {
            self.opacity = d.opacity;
        }
        if !(1_000..=200_000).contains(&self.scrollback) {
            self.scrollback = d.scrollback;
        }
        self
    }
}

/// The per-user settings file (~/Library/Application Support/jamalterm/settings.json).
pub fn default_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("jamalterm")
        .join("settings.json")
}

/// Reads settings; a missing or damaged file gives the defaults.
pub fn load(path: &Path) -> Settings {
    std::fs::read(path)
        .ok()
        .and_then(|b| serde_json::from_slice::<Settings>(&b).ok())
        .unwrap_or_default()
        .normalized()
}

/// Writes atomically (a crash can't leave half a file), private to the user.
pub fn save(path: &Path, s: &Settings) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(&s.clone().normalized())?)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600))?;
    }
    std::fs::rename(tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_or_damaged_file_gives_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("settings.json");
        assert_eq!(load(&p), Settings::default());
        std::fs::write(&p, "{nope").unwrap();
        assert_eq!(load(&p), Settings::default());
    }

    #[test]
    fn reads_a_go_era_file_and_keeps_unknown_values() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("settings.json");
        std::fs::write(&p, r#"{"theme":"nord","fontSize":99,"aiProvider":"openai","hostColors":[{"pattern":"prod-*","color":"red"}]}"#).unwrap();
        let s = load(&p);
        assert_eq!(s.theme, "nord");
        assert_eq!(s.font_size, 14, "out of range: default");
        assert_eq!(
            s.ai_provider, "openai",
            "settings for later features survive"
        );
        assert_eq!(s.host_colors[0].pattern, "prod-*");
        assert!(s.cursor_blink, "missing fields keep their defaults");
        save(&p, &s).unwrap();
        let raw = std::fs::read_to_string(&p).unwrap();
        assert!(raw.contains(r#""aiProvider": "openai""#) && raw.contains(r#""fontSize": 14"#));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&p).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
    }
}
