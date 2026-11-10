//! jamalterm: a macOS terminal with a free-floating cursor (browse mode) and
//! AI built in. This is the Rust version (4.x); see docs/DESIGN.md.

mod alert;
mod app;
mod browse;
mod commands;
mod find;
mod fonts;
mod input;
mod marks;
#[cfg(target_os = "macos")]
mod menu;
mod palette;
mod panes;
mod prefs;
mod restore;
mod session;
mod settings;
mod shell;
mod smoke;
mod tabdrag;
mod themes;
mod view;

fn main() -> eframe::Result<()> {
    let start = app::Startup::load();
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("jamalterm")
        .with_inner_size([1100.0, 700.0])
        .with_min_inner_size([480.0, 300.0]);
    // The window comes back where it was, at its size.
    if let Some(f) = start.restored.first().and_then(|w| w.frame) {
        viewport = viewport
            .with_position([f.x as f32, f.y as f32])
            .with_inner_size([f.w.max(480) as f32, f.h.max(300) as f32]);
    }
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    eframe::run_native(
        "jamalterm",
        options,
        Box::new(|cc| Ok(Box::new(app::App::new(cc, start)))),
    )
}
