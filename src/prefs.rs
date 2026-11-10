//! The Settings window (⌘,): a row of panes along the top like Terminal's
//! and Safari's, and the window title names the pane. Every change applies
//! at once: there's no Save or Cancel. Options most people never change sit
//! under a closed Advanced section at the bottom of each pane. Hovering a
//! theme shows it in every window; moving away puts yours back.
//!
//! Only settings that work in this version are shown; the rest of 3.x's
//! (AI, Privacy, Updates) come back with their features. Their fields stay
//! in settings.json untouched.

use crate::palette::{Pane, TOGGLES};
use crate::settings::Settings;
use egui::{RichText, Ui};

pub struct Out {
    /// Something changed (save and apply it).
    pub changed: bool,
    /// The theme under the pointer, to show for now.
    pub preview: Option<&'static str>,
}

pub fn show(ui: &mut Ui, s: &mut Settings, pane: &mut Pane) -> Out {
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
        for (i, t) in crate::themes::themes().into_iter().enumerate() {
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
