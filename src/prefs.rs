//! The Settings window (⌘,): a row of panes along the top like Terminal's
//! and Safari's, and the window title names the pane. Every change applies
//! at once: there's no Save or Cancel. Options most people never change sit
//! under a closed Advanced section at the bottom of each pane. Hovering a
//! theme shows it in every window; moving away puts yours back.
//!
//! Only settings that work in this version are shown; the rest of 3.x's
//! (AI, Privacy, Updates) come back with their features. Their fields stay
//! in settings.json untouched.

use crate::ai;
use crate::keys::Keys;
use crate::palette::{Pane, TOGGLES};
use crate::settings::Settings;
use egui::{RichText, Ui};

pub struct Out {
    /// Something changed (save and apply it).
    pub changed: bool,
    /// The theme under the pointer, to show for now.
    pub preview: Option<&'static str>,
}

/// What the AI pane is in the middle of: the key being typed, and the last
/// message ("Saved.", "Found Ollama…").
#[derive(Default)]
pub struct State {
    pub key: String,
    pub message: String,
}

pub fn show(
    ui: &mut Ui,
    s: &mut Settings,
    pane: &mut Pane,
    keys: &mut Keys,
    st: &mut State,
) -> Out {
    let before = s.clone();
    let mut preview = None;

    // The panes, as a toolbar.
    ui.horizontal(|ui| {
        for p in Pane::ALL {
            if ui
                .selectable_label(*pane == p, RichText::new(p.name()).size(15.0))
                .clicked()
            {
                *pane = p;
            }
        }
    });
    ui.separator();
    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.add_space(4.0);
        match pane {
            Pane::Appearance => appearance(ui, s, &mut preview),
            Pane::Shell => shell(ui, s),
            Pane::Ai => ai_pane(ui, s, keys, st),
            Pane::Privacy => privacy(ui, s),
        }
    });
    Out {
        changed: *s != before,
        preview,
    }
}

fn toggles(ui: &mut Ui, s: &mut Settings, pane: Pane, advanced: bool) {
    for t in TOGGLES
        .iter()
        .filter(|t| t.pane == pane && t.advanced == advanced)
    {
        ui.checkbox((t.field)(s), t.label);
    }
}

fn appearance(ui: &mut Ui, s: &mut Settings, preview: &mut Option<&'static str>) {
    ui.label(RichText::new("Theme").strong());
    egui::Grid::new("themes").num_columns(2).show(ui, |ui| {
        let specials = crate::palette::special_themes();
        for (i, t) in crate::themes::themes()
            .into_iter()
            .chain(specials)
            .enumerate()
        {
            let current = s.theme == t.id;
            // A swatch of the theme's own colors, then its name (and a ✓ in
            // words for the current one: not color alone).
            let r = ui
                .horizontal(|ui| {
                    let (rect, _) =
                        ui.allocate_exact_size(egui::vec2(34.0, 18.0), egui::Sense::hover());
                    let p = ui.painter();
                    p.rect_filled(rect, 3.0, t.background);
                    for (k, c) in [t.ansi[1], t.ansi[2], t.ansi[4], t.foreground]
                        .iter()
                        .enumerate()
                    {
                        let x = rect.left() + 5.0 + k as f32 * 7.0;
                        p.rect_filled(
                            egui::Rect::from_min_size(
                                egui::pos2(x, rect.top() + 6.0),
                                egui::vec2(5.0, 6.0),
                            ),
                            1.0,
                            *c,
                        );
                    }
                    let label = if current {
                        format!("{} (current)", t.name)
                    } else {
                        t.name.to_string()
                    };
                    ui.selectable_label(current, label)
                })
                .inner;
            if r.hovered() {
                *preview = Some(t.id);
            }
            if r.clicked() {
                s.theme = t.id.to_string();
            }
            if i % 2 == 1 {
                ui.end_row();
            }
        }
    });
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.label("Font size");
        ui.add(egui::Slider::new(&mut s.font_size, 8..=48).suffix(" pt"));
    });
    ui.horizontal(|ui| {
        ui.label("Cursor");
        for (id, name) in [
            ("block", "Block"),
            ("bar", "Bar"),
            ("underline", "Underline"),
        ] {
            if ui.radio(s.cursor_style == id, name).clicked() {
                s.cursor_style = id.into();
            }
        }
    });
    ui.horizontal(|ui| {
        ui.label("Cursor effect");
        for (id, name) in [("none", "None"), ("smooth", "Smooth"), ("glow", "Glow")] {
            if ui.radio(s.cursor_effect == id, name).clicked() {
                s.cursor_effect = id.into();
            }
        }
    });
    toggles(ui, s, Pane::Appearance, false);
    ui.add_space(8.0);
    egui::CollapsingHeader::new("Advanced")
        .default_open(false)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label("Line height");
                ui.add(egui::Slider::new(&mut s.line_height, 1.0..=2.0).step_by(0.05));
            });
            toggles(ui, s, Pane::Appearance, true);
        });
}

fn ai_pane(ui: &mut Ui, s: &mut Settings, keys: &mut Keys, st: &mut State) {
    toggles(ui, s, Pane::Ai, false);
    if !s.ai_enabled {
        return;
    }
    ui.add_space(8.0);
    ui.label(RichText::new("Assistant").strong());
    for p in ai::PROVIDERS {
        if ui.radio(s.ai_provider == p.id, p.label).clicked() && s.ai_provider != p.id {
            s.ai_provider = p.id.into();
            st.key.clear();
            st.message.clear();
        }
    }
    let p = ai::provider(&s.ai_provider);
    ui.add_space(8.0);
    if p.local {
        ui.horizontal(|ui| {
            ui.label("Server");
            ui.add(egui::TextEdit::singleline(&mut s.local_url).desired_width(240.0));
            if ui
                .button("Find")
                .on_hover_text("Look for Ollama and LM Studio on this Mac")
                .clicked()
            {
                st.message = find_local(s);
            }
        });
    } else {
        let env = crate::keys::ENV
            .iter()
            .find(|(id, _)| *id == p.id)
            .map(|e| e.1)
            .unwrap_or("");
        ui.horizontal(|ui| {
            ui.label("API key");
            let hint = if keys.saved(p.id) {
                "saved (encrypted on this computer)".to_string()
            } else {
                p.key_hint.to_string()
            };
            let edit = ui.add(
                egui::TextEdit::singleline(&mut st.key)
                    .password(true)
                    .hint_text(hint)
                    .desired_width(240.0),
            );
            let enter = edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if (ui.button("Save").clicked() || enter) && !st.key.trim().is_empty() {
                st.message = match keys.set(p.id, &st.key) {
                    Ok(()) => "Saved, encrypted in jamalterm's key file.".into(),
                    Err(e) => e,
                };
                st.key.clear();
            }
            if keys.saved(p.id) && ui.button("Remove").clicked() {
                st.message = match keys.set(p.id, "") {
                    Ok(()) => "Removed.".into(),
                    Err(e) => e,
                };
            }
        });
        ui.weak(format!("Make one at {}. {}", p.key_where, p.billing));
        if !keys.saved(p.id) && keys.get(p.id).is_some() {
            ui.weak(format!("Using ${env} from your environment."));
        }
        if keys.unreadable {
            ui.label(
                "⚠ Saved keys couldn't be opened on this computer (copied from another Mac?). Saving a key starts a new key file.",
            );
        }
    }
    if let Some(m) = ai::model_setting_mut(s, p.id) {
        ui.horizontal(|ui| {
            ui.label("Model");
            ui.add(
                egui::TextEdit::singleline(m)
                    .hint_text(format!("automatic: {}", p.auto_model))
                    .desired_width(260.0),
            );
        });
    }
    if !st.message.is_empty() {
        ui.label(st.message.as_str());
    }
}

/// Looks for a local model server (Ollama, LM Studio) and uses the first
/// that answers.
fn find_local(s: &mut Settings) -> String {
    for (name, url) in [
        ("Ollama", "http://localhost:11434/v1"),
        ("LM Studio", "http://localhost:1234/v1"),
    ] {
        let ep = ai::Endpoint {
            provider: "local",
            base: url.into(),
            key: "local".into(),
            model: String::new(),
        };
        if let Ok(models) = ai::compat::list_models(&ep) {
            s.local_url = url.into();
            return format!("Found {name} with {} model(s).", models.len());
        }
    }
    "Neither Ollama nor LM Studio is running on this Mac.".into()
}

fn privacy(ui: &mut Ui, s: &mut Settings) {
    toggles(ui, s, Pane::Privacy, false);
    egui::CollapsingHeader::new("Advanced")
        .default_open(false)
        .show(ui, |ui| toggles(ui, s, Pane::Privacy, true));
    ui.add_space(8.0);
    for line in [
        "With each AI question go your recent commands, their exit codes and (if on above) their output. Things that look like keys, tokens and passwords are replaced with [hidden secret] first, and the panel says how many.",
        "Local models never send anything off this Mac.",
        "API keys are kept in one encrypted file next to the settings, readable only by you and only on this computer, never in the Keychain: nothing asks for your password.",
        "jamalterm never logs or sends your commands, output, questions or keys anywhere else. The problem log keeps only errors, scrubbed (secrets, your home folder, email addresses and this computer's name removed), and stays on this Mac: Help → Copy Diagnostics shows all of it before anything is copied.",
    ] {
        ui.label(line);
        ui.add_space(4.0);
    }
}

fn shell(ui: &mut Ui, s: &mut Settings) {
    toggles(ui, s, Pane::Shell, false);
    ui.add_space(8.0);
    egui::CollapsingHeader::new("Advanced")
        .default_open(false)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label("Shell program");
                ui.add(
                    egui::TextEdit::singleline(&mut s.shell)
                        .hint_text("your login shell")
                        .desired_width(220.0),
                );
            });
            ui.horizontal(|ui| {
                ui.label("Scrollback");
                ui.add(
                    egui::DragValue::new(&mut s.scrollback)
                        .range(1000..=200_000)
                        .speed(100)
                        .suffix(" lines"),
                );
            });
            ui.horizontal(|ui| {
                ui.label("Prompt keys");
                for (id, name) in [("emacs", "Emacs (most shells)"), ("vi", "vi")] {
                    if ui.radio(s.prompt_keys == id, name).clicked() {
                        s.prompt_keys = id.into();
                    }
                }
            });
            toggles(ui, s, Pane::Shell, true);
            ui.weak("The shell program, scrollback and shell integration apply to new tabs.");
        });
}
