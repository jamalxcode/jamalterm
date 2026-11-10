//! Fonts: JetBrains Mono (regular and bold) for the terminal, with the Nerd
//! Font symbols as a fallback so Powerlevel10k, eza and starship icons show up
//! with any font.

use egui::{FontData, FontDefinitions, FontFamily};
use std::sync::Arc;

pub fn install(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let add = |fonts: &mut FontDefinitions, name: &str, bytes: &'static [u8]| {
        fonts
            .font_data
            .insert(name.into(), Arc::new(FontData::from_static(bytes)));
    };
    add(
        &mut fonts,
        "jbm",
        include_bytes!("../assets/fonts/JetBrainsMono-Regular.ttf"),
    );
    add(
        &mut fonts,
        "jbm-bold",
        include_bytes!("../assets/fonts/JetBrainsMono-Bold.ttf"),
    );
    add(
        &mut fonts,
        "nerd",
        include_bytes!("../assets/fonts/SymbolsNerdFontMono.ttf"),
    );
    let mono = fonts.families.entry(FontFamily::Monospace).or_default();
    mono.insert(0, "jbm".into());
    mono.insert(1, "nerd".into());
    let fallback: Vec<String> = fonts.families[&FontFamily::Monospace][2..].to_vec();
    let mut bold = vec!["jbm-bold".to_string(), "nerd".to_string()];
    bold.extend(fallback);
    fonts
        .families
        .insert(FontFamily::Name("mono-bold".into()), bold);
    ctx.set_fonts(fonts);
}
