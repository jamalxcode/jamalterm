//! The AI panel (⌘I): a raised, violet-tinted card beside the terminal, so
//! it never reads as part of it. Questions go with recent commands and
//! (unless switched off) their output, secrets hidden. Commands in answers
//! get Run and Insert buttons; a command the AI wants to run waits for a
//! click on Run, and a risky one says why and needs a second click (Run
//! Anyway). Warnings are words and ⚠, not color alone.

use crate::ai::{self, Event, History, Running, risk};
use egui::{Color32, RichText, Ui};
use std::sync::{Arc, Mutex};
use std::time::Instant;

pub enum Item {
    User(String),
    Answer(String),
    Tool(Card),
    /// A quiet note ("2 secrets were hidden…").
    Note(String),
    Error(String),
}

pub struct Card {
    pub command: String,
    pub risk: risk::Risk,
    /// Risky: the first click on Run turned it into Run Anyway.
    pub armed: bool,
    pub state: CardState,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CardState {
    /// Waiting for Run or Don't Run.
    Asking,
    /// Running in a pane; the AI hears back when it finishes.
    Running {
        pane: u64,
        before: usize,
        since: Instant,
    },
    Done(String),
}

/// What the user did in the panel, for the app to carry out.
#[derive(Debug, Clone, PartialEq)]
pub enum Act {
    Ask(String),
    /// Type a command at the prompt (and run it).
    Type {
        command: String,
        run: bool,
    },
    /// The AI's command: run it (true) or not.
    Decide(bool),
    Stop,
    Close,
    Settings,
}

#[derive(Default)]
pub struct Chat {
    pub items: Vec<Item>,
    pub input: String,
    pub running: Option<Running>,
    pub history: Arc<Mutex<History>>,
    pub model: String,
    pub focus_input: bool,
    /// The terminal text sent with the last question (to spot planted
    /// commands).
    pub shown: String,
    /// Risky code-block commands armed for Run Anyway: (item, block).
    armed: Vec<(usize, usize)>,
}

impl Chat {
    /// Takes the AI's events. Returns tool calls to carry out.
    pub fn poll(&mut self) -> Vec<(String, bool)> {
        let mut tools = vec![];
        let Some(r) = &self.running else {
            return tools;
        };
        let mut end = false;
        while let Ok(e) = r.events.try_recv() {
            match e {
                Event::Model(m) => self.model = m,
                Event::Hidden(n) => self.items.push(Item::Note(format!(
                    "{n} secret{} hidden from what was sent.",
                    if n == 1 { " was" } else { "s were" }
                ))),
                Event::Text(t) => match self.items.last_mut() {
                    Some(Item::Answer(a)) => a.push_str(&t),
                    _ => self.items.push(Item::Answer(t)),
                },
                Event::Tool { command, run } => tools.push((command, run)),
                Event::Done => end = true,
                Event::Error(e) => {
                    self.items.push(Item::Error(e));
                    end = true;
                }
            }
        }
        if end {
            self.running = None;
        }
        tools
    }

    /// Answers the AI's tool call.
    pub fn reply(&self, text: String, is_error: bool) {
        if let Some(r) = &self.running {
            let _ = r.replies.send(ai::Reply { text, is_error });
        }
    }

    /// The card waiting for an answer, if any.
    pub fn card_mut(&mut self) -> Option<&mut Card> {
        self.items.iter_mut().rev().find_map(|i| match i {
            Item::Tool(c) if !matches!(c.state, CardState::Done(_)) => Some(c),
            _ => None,
        })
    }

    pub fn busy(&self) -> bool {
        self.running.is_some()
    }
}

/// The panel's tint: the terminal background mixed with violet, a little
/// lighter, so it reads as its own raised card.
pub fn tint(bg: Color32, light: bool) -> Color32 {
    let violet = Color32::from_rgb(124, 92, 255);
    let mix = |a: u8, b: u8, t: f32| (a as f32 * (1.0 - t) + b as f32 * t) as u8;
    let t = if light { 0.08 } else { 0.16 };
    let lift = if light { 0 } else { 10 };
    Color32::from_rgb(
        mix(bg.r(), violet.r(), t).saturating_add(lift),
        mix(bg.g(), violet.g(), t).saturating_add(lift),
        mix(bg.b(), violet.b(), t).saturating_add(lift),
    )
}

/// Splits an answer into text and ```code``` blocks: (is_code, text).
pub fn pieces(answer: &str) -> Vec<(bool, String)> {
    let mut out = vec![];
    let mut code = false;
    let mut cur = String::new();
    for line in answer.split_inclusive('\n') {
        if line.trim_start().starts_with("```") {
            let t = cur.trim_end_matches('\n').to_string();
            if !t.trim().is_empty() {
                out.push((code, t));
            }
            cur.clear();
            code = !code;
            continue;
        }
        cur.push_str(line);
    }
    let t = cur.trim_end_matches('\n').to_string();
    if !t.trim().is_empty() {
        out.push((code, t));
    }
    out
}

pub struct Look<'a> {
    pub provider: &'static str,
    pub ready: Result<(), String>,
    pub enabled: bool,
    pub share_output: bool,
    pub last_failed: Option<&'a str>,
}

/// Draws the panel. Returns what the user did.
pub fn show(ui: &mut Ui, chat: &mut Chat, look: &Look) -> Vec<Act> {
    let mut acts = vec![];
    ui.horizontal(|ui| {
        ui.label(RichText::new("AI").strong().size(15.0));
        let who = ai::provider(look.provider).name;
        let model = if chat.model.is_empty() {
            String::new()
        } else {
            format!(" · {}", chat.model)
        };
        ui.weak(format!("{who}{model}"));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .small_button("Close")
                .on_hover_text("Hide the panel (⌘I)")
                .clicked()
            {
                acts.push(Act::Close);
            }
        });
    });
    ui.separator();
    if !look.enabled {
        ui.label("AI features are off.");
        if ui.button("Turn them on in Settings…").clicked() {
            acts.push(Act::Settings);
        }
        return acts;
    }
    if let Err(why) = &look.ready {
        ui.label(why);
        if ui.button("Open Settings…").clicked() {
            acts.push(Act::Settings);
        }
        return acts;
    }

    let input_h = 64.0;
    egui::ScrollArea::vertical()
        .max_height((ui.available_height() - input_h).max(80.0))
        .stick_to_bottom(true)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            if chat.items.is_empty() {
                ui.weak("Ask about a command, an error or its output. Recent commands go with the question");
                ui.weak(if look.share_output {
                    "(with their output; secrets are hidden first)."
                } else {
                    "(without their output: switched off in Settings → Privacy)."
                });
                if let Some(cmd) = look.last_failed {
                    ui.add_space(6.0);
                    if ui.button(format!("Why did “{cmd}” fail?")).clicked() {
                        acts.push(Act::Ask(format!("Why did `{cmd}` fail, and how do I fix it?")));
                    }
                }
            }
            let mut i = 0;
            while i < chat.items.len() {
                item(ui, chat, i, &mut acts);
                i += 1;
            }
            if chat.busy() && chat.card_mut().is_none() {
                ui.weak("…");
            }
        });

    ui.separator();
    ui.horizontal(|ui| {
        // ⏎ sends (taken before the box would add a line); ⇧⏎ adds a line.
        let id = ui.make_persistent_id("ai-input");
        let enter = ui.memory(|m| m.has_focus(id))
            && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter));
        let edit = ui.add(
            egui::TextEdit::multiline(&mut chat.input)
                .id(id)
                .hint_text("Ask the AI (⏎ to send, ⇧⏎ for a new line)")
                .desired_rows(2)
                .desired_width(ui.available_width() - 56.0),
        );
        if std::mem::take(&mut chat.focus_input) {
            edit.request_focus();
        }
        if chat.busy() {
            if ui.button("Stop").clicked() {
                acts.push(Act::Stop);
            }
        } else if (ui.button("Ask").clicked() || enter) && !chat.input.trim().is_empty() {
            let q = chat.input.trim().to_string();
            chat.input.clear();
            acts.push(Act::Ask(q));
        }
    });
    acts
}

fn item(ui: &mut Ui, chat: &mut Chat, i: usize, acts: &mut Vec<Act>) {
    ui.add_space(4.0);
    match &mut chat.items[i] {
        Item::User(q) => {
            ui.label(RichText::new(format!("You: {q}")).strong());
        }
        Item::Note(n) => {
            ui.weak(n.as_str());
        }
        Item::Error(e) => {
            ui.label(RichText::new(format!("⚠ {e}")).color(ui.visuals().warn_fg_color));
        }
        Item::Answer(a) => {
            let parts = pieces(a);
            for (b, (code, text)) in parts.into_iter().enumerate() {
                if !code {
                    ui.add(egui::Label::new(text).wrap().selectable(true));
                    continue;
                }
                egui::Frame::group(ui.style()).show(ui, |ui| {
                    ui.add(egui::Label::new(RichText::new(&text).monospace()).selectable(true));
                    let r = risk::check(&text, &chat.shown);
                    let armed = chat.armed.contains(&(i, b));
                    if armed && let Some(level) = r.level {
                        warning(ui, level, &r.reasons);
                    }
                    ui.horizontal(|ui| {
                        let run = if armed { "Run Anyway" } else { "Run" };
                        if ui
                            .button(run)
                            .on_hover_text("Run it in the terminal")
                            .clicked()
                        {
                            if r.level.is_some() && !armed {
                                chat.armed.push((i, b));
                            } else {
                                acts.push(Act::Type {
                                    command: text.clone(),
                                    run: true,
                                });
                            }
                        }
                        if ui
                            .button("Insert")
                            .on_hover_text("Type it at the prompt without running it")
                            .clicked()
                        {
                            acts.push(Act::Type {
                                command: text.clone(),
                                run: false,
                            });
                        }
                    });
                });
            }
        }
        Item::Tool(card) => {
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.label("The AI wants to run:");
                ui.add(egui::Label::new(RichText::new(&card.command).monospace()).selectable(true));
                match &card.state {
                    CardState::Asking => {
                        if card.armed
                            && let Some(level) = card.risk.level
                        {
                            warning(ui, level, &card.risk.reasons);
                        }
                        ui.horizontal(|ui| {
                            let label = if card.armed { "Run Anyway" } else { "Run" };
                            if ui.button(label).clicked() {
                                if card.risk.level.is_some() && !card.armed {
                                    card.armed = true;
                                } else {
                                    acts.push(Act::Decide(true));
                                }
                            }
                            if ui.button("Don't Run").clicked() {
                                acts.push(Act::Decide(false));
                            }
                        });
                    }
                    CardState::Running { .. } => {
                        ui.weak("… running");
                    }
                    CardState::Done(what) => {
                        ui.weak(what.as_str());
                    }
                }
            });
        }
    }
}

fn warning(ui: &mut Ui, level: risk::Level, reasons: &[&str]) {
    ui.label(
        RichText::new(format!("⚠ {}", level.words()))
            .strong()
            .color(ui.visuals().warn_fg_color),
    );
    for r in reasons {
        ui.label(format!("• It {r}."));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn answers_split_into_text_and_commands() {
        let a = "Try this:\n```sh\nls -la\n```\nThen:\n```\ngit status\n```";
        assert_eq!(
            pieces(a),
            [
                (false, "Try this:".to_string()),
                (true, "ls -la".to_string()),
                (false, "Then:".to_string()),
                (true, "git status".to_string())
            ]
        );
        assert_eq!(pieces("just text"), [(false, "just text".to_string())]);
        // Still streaming: an open block is code so far.
        assert_eq!(pieces("```sh\nls")[0], (true, "ls".to_string()));
    }

    #[test]
    fn the_tint_is_its_own_color() {
        let bg = Color32::from_rgb(20, 20, 28);
        let t = tint(bg, false);
        assert!(t.b() > bg.b() + 20 && t.r() > bg.r(), "violet, lighter");
        let light = Color32::from_rgb(250, 250, 250);
        assert_ne!(tint(light, true), light);
    }

    #[test]
    fn events_fill_the_chat() {
        let (tx, rx) = std::sync::mpsc::channel();
        let (rtx, _rrx) = std::sync::mpsc::channel();
        let mut c = Chat {
            running: Some(Running {
                events: rx,
                replies: rtx,
                cancel: Default::default(),
            }),
            ..Default::default()
        };
        for e in [
            Event::Model("claude-opus-5-5".into()),
            Event::Hidden(2),
            Event::Text("Hel".into()),
            Event::Text("lo".into()),
            Event::Tool {
                command: "ls".into(),
                run: true,
            },
            Event::Done,
        ] {
            tx.send(e).unwrap();
        }
        let tools = c.poll();
        assert_eq!(tools, [("ls".to_string(), true)]);
        assert_eq!(c.model, "claude-opus-5-5");
        assert!(matches!(&c.items[0], Item::Note(n) if n.starts_with("2 secrets were")));
        assert!(matches!(&c.items[1], Item::Answer(a) if a == "Hello"));
        assert!(!c.busy());
    }
}
