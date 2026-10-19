//! Vendored Roboto fonts and the font definitions for the interface.
//!
//! The egui default fonts stay off, because their licenses are off the
//! allowlist. This crate embeds the Roboto files, so no file is read at run
//! time. See `SOURCES.md` next to the font files for provenance.

use std::sync::Arc;

use egui::{FontData, FontDefinitions, FontFamily};

/// The Roboto Regular font file.
pub const REGULAR: &[u8] = include_bytes!("../assets/fonts/Roboto-Regular.ttf");

/// The Roboto Medium font file.
pub const MEDIUM: &[u8] = include_bytes!("../assets/fonts/Roboto-Medium.ttf");

/// The Roboto Bold font file.
pub const BOLD: &[u8] = include_bytes!("../assets/fonts/Roboto-Bold.ttf");

/// Build the font definitions for the interface.
///
/// Proportional lists Regular, Medium, then Bold. Monospace holds Regular only.
/// `Roboto Medium` and `Roboto Bold` are named families. egui 0.36.2 has no
/// weight selection, so those weights come only through them.
#[must_use]
pub fn definitions() -> FontDefinitions {
    let mut fonts = FontDefinitions::empty();

    for (name, bytes) in [
        ("roboto-regular", REGULAR),
        ("roboto-medium", MEDIUM),
        ("roboto-bold", BOLD),
    ] {
        fonts
            .font_data
            .insert(name.to_owned(), Arc::new(FontData::from_static(bytes)));
    }

    fonts.families.insert(
        FontFamily::Proportional,
        vec![
            "roboto-regular".to_owned(),
            "roboto-medium".to_owned(),
            "roboto-bold".to_owned(),
        ],
    );
    // This family has no monospace font available, so Regular is the fallback.
    fonts
        .families
        .insert(FontFamily::Monospace, vec!["roboto-regular".to_owned()]);
    fonts.families.insert(
        FontFamily::Name("Roboto Medium".into()),
        vec!["roboto-medium".to_owned()],
    );
    fonts.families.insert(
        FontFamily::Name("Roboto Bold".into()),
        vec!["roboto-bold".to_owned()],
    );

    fonts
}

/// Install the Roboto fonts on the given context.
pub fn install(ctx: &egui::Context) {
    ctx.set_fonts(definitions());
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Color32, FontId, RawInput};

    #[test]
    fn the_definitions_hold_the_three_fonts() {
        let fonts = definitions();

        for name in ["roboto-regular", "roboto-medium", "roboto-bold"] {
            assert!(fonts.font_data.contains_key(name), "missing {name}");
        }
        for family in [FontFamily::Proportional, FontFamily::Monospace] {
            assert!(
                !fonts.families[&family].is_empty(),
                "empty host list for {family:?}"
            );
        }
    }

    #[test]
    fn the_fonts_install_and_shape_text() {
        let ctx = egui::Context::default();
        install(&ctx);
        // The font atlas is built in the first frame, so one frame must run.
        ctx.run_ui(RawInput::default(), |_| {})
            .drop_without_applying_deltas();

        let shape = |family| {
            let font = FontId::new(14.0, family);
            ctx.fonts_mut(|fonts| fonts.layout_no_wrap("Tarragon".to_owned(), font, Color32::WHITE))
        };

        let galley = shape(FontFamily::Proportional);
        let glyphs: usize = galley.rows.iter().map(|row| row.row.glyphs.len()).sum();
        assert_eq!(glyphs, 8, "expected one glyph for each letter");
        assert!(galley.rect.width() > 0.0, "text has no width");

        let mono = shape(FontFamily::Monospace);
        let mono_glyphs: usize = mono.rows.iter().map(|row| row.row.glyphs.len()).sum();
        assert_eq!(mono_glyphs, 8, "expected one glyph for each letter");
        assert!(mono.rect.width() > 0.0, "Monospace has no width");
    }

    #[test]
    fn the_monospace_family_uses_roboto_regular() {
        let fonts = definitions();
        let list = &fonts.families[&FontFamily::Monospace];

        assert_eq!(list.first().map(String::as_str), Some("roboto-regular"));
    }
}
