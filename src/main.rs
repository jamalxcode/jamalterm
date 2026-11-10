//! jamalterm: a macOS terminal with a free-floating cursor (browse mode) and
//! AI built in. This is the Rust version (4.x); see docs/DESIGN.md.

mod app;
mod browse;
mod find;
mod fonts;
mod input;
mod marks;
mod session;
mod settings;
mod shell;
mod smoke;
mod themes;
mod view;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("jamalterm")
            .with_inner_size([1100.0, 700.0])
            .with_min_inner_size([480.0, 300.0]),
        ..Default::default()
    };
    eframe::run_native(
        "jamalterm",
        options,
        Box::new(|cc| Ok(Box::new(app::App::new(cc)))),
    )
}
