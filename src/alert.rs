//! Asking before something running is ended: closing a pane, tab or window,
//! or quitting, while a program other than the shell is in front.
//!
//! On a Mac it's a native alert shown as a sheet on the window, worded as in
//! Terminal, with **Cancel as the default** (⏎ and Esc), following Apple's
//! advice not to make a destructive action the default. Elsewhere, and in
//! the self-test (which can't click a native alert), the same question is
//! asked inside the window.

use std::sync::mpsc;

/// What's about to close.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Closing {
    /// A pane (by id) in a window.
    Pane {
        win: u64,
        pane: u64,
    },
    /// A tab (by one of its panes) in a window.
    Tab {
        win: u64,
        pane: u64,
    },
    Window {
        win: u64,
    },
    Quit,
}

impl Closing {
    /// Terminal's words.
    pub fn question(&self) -> &'static str {
        match self {
            Closing::Pane { .. } | Closing::Tab { .. } => {
                "Do you want to terminate running processes in this tab?"
            }
            Closing::Window { .. } => "Do you want to terminate running processes in this window?",
            Closing::Quit => "Do you want to quit jamalterm?",
        }
    }

    /// What will end, in words: "Closing this tab will end vim."
    pub fn detail(&self, programs: &[String]) -> String {
        let what = match self {
            Closing::Pane { .. } => "Closing this pane",
            Closing::Tab { .. } => "Closing this tab",
            Closing::Window { .. } => "Closing this window",
            Closing::Quit => "Quitting",
        };
        let list = match programs {
            [] => "what's running".to_string(),
            [one] => one.clone(),
            [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
        };
        format!("{what} will end {list}.")
    }
}

/// A question waiting for its answer.
pub struct Pending {
    pub closing: Closing,
    pub detail: String,
    /// The native alert's answer (true: terminate); None when it's asked in
    /// the window instead.
    pub native: Option<mpsc::Receiver<bool>>,
}

/// Shows the native alert as a sheet on the window in front. None if it
/// couldn't be shown (then ask in the window).
#[cfg(target_os = "macos")]
pub fn ask_native(
    closing: Closing,
    detail: &str,
    ctx: &egui::Context,
) -> Option<mpsc::Receiver<bool>> {
    use block2::RcBlock;
    use objc2::MainThreadMarker;
    use objc2_app_kit::{
        NSAlert, NSAlertSecondButtonReturn, NSAlertStyle, NSApplication, NSModalResponse,
    };
    use objc2_foundation::NSString;

    let mtm = MainThreadMarker::new()?;
    let app = NSApplication::sharedApplication(mtm);
    let window = app.keyWindow().or_else(|| app.mainWindow())?;
    let alert = NSAlert::new(mtm);
    alert.setAlertStyle(NSAlertStyle::Warning);
    alert.setMessageText(&NSString::from_str(closing.question()));
    alert.setInformativeText(&NSString::from_str(detail));
    // The first button is the default (⏎); one titled Cancel also answers Esc.
    alert.addButtonWithTitle(&NSString::from_str("Cancel"));
    alert.addButtonWithTitle(&NSString::from_str(if closing == Closing::Quit {
        "Quit"
    } else {
        "Terminate"
    }));
    let (tx, rx) = mpsc::channel();
    let ctx = ctx.clone();
    let done = RcBlock::new(move |answer: NSModalResponse| {
        let _ = tx.send(answer == NSAlertSecondButtonReturn);
        ctx.request_repaint();
    });
    alert.beginSheetModalForWindow_completionHandler(&window, Some(&done));
    Some(rx)
}

#[cfg(not(target_os = "macos"))]
pub fn ask_native(_: Closing, _: &str, _: &egui::Context) -> Option<mpsc::Receiver<bool>> {
    None
}

/// The question inside the window. Returns the answer once given.
pub fn ask_in_window(ctx: &egui::Context, p: &Pending) -> Option<bool> {
    let mut answer = None;
    egui::Modal::new(egui::Id::unique("close-question")).show(ctx, |ui| {
        ui.set_max_width(380.0);
        ui.label(egui::RichText::new(p.closing.question()).strong());
        ui.label(&p.detail);
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            let end = if p.closing == Closing::Quit {
                "Quit"
            } else {
                "Terminate"
            };
            if ui.button(end).clicked() {
                answer = Some(true);
            }
            // Cancel is the default: ⏎ and Esc.
            let cancel = ui.button("Cancel");
            cancel.request_focus();
            if cancel.clicked() {
                answer = Some(false);
            }
        });
        if ui.input(|i| i.key_pressed(egui::Key::Escape) || i.key_pressed(egui::Key::Enter)) {
            answer = Some(false);
        }
    });
    answer
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words() {
        let p = Closing::Pane { win: 1, pane: 2 };
        assert_eq!(p.detail(&["vim".into()]), "Closing this pane will end vim.");
        assert_eq!(
            Closing::Window { win: 1 }.detail(&["vim".into(), "ssh".into(), "npm".into()]),
            "Closing this window will end vim, ssh and npm."
        );
        assert_eq!(
            Closing::Quit.detail(&[]),
            "Quitting will end what's running."
        );
        assert!(
            Closing::Window { win: 1 }
                .question()
                .contains("in this window")
        );
    }
}
