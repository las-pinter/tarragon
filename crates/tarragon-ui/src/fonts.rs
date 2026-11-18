//! Vendored Roboto fonts and the font definitions for the interface.
//!
//! The egui default fonts stay off, because their licenses are off the
//! allowlist. This crate embeds the Roboto files, so no file is read at run
//! time. See `SOURCES.md` next to the font files for provenance.

use std::collections::HashSet;
use std::sync::Arc;

use egui::epaint::text::{FontInsert, FontPriority, InsertFontFamily};
use egui::{FontData, FontDefinitions, FontFamily};
use skrifa::FontRef;

/// The Roboto Regular font file.
pub const REGULAR: &[u8] = include_bytes!("../assets/fonts/Roboto-Regular.ttf");

/// The Roboto Medium font file.
pub const MEDIUM: &[u8] = include_bytes!("../assets/fonts/Roboto-Medium.ttf");

/// The Roboto Bold font file.
pub const BOLD: &[u8] = include_bytes!("../assets/fonts/Roboto-Bold.ttf");

/// The named egui family that holds the Medium weight.
pub const MEDIUM_FAMILY: &str = "Roboto Medium";

/// The named egui family that holds the Bold weight.
pub const BOLD_FAMILY: &str = "Roboto Bold";

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
        FontFamily::Name(MEDIUM_FAMILY.into()),
        vec!["roboto-medium".to_owned()],
    );
    fonts.families.insert(
        FontFamily::Name(BOLD_FAMILY.into()),
        vec!["roboto-bold".to_owned()],
    );

    fonts
}

/// The context flag that records a completed font install.
fn guard_id() -> egui::Id {
    egui::Id::new("tarragon-ui/fonts-installed")
}

/// Install the Roboto fonts on the given context.
///
/// Only the first call installs. A later call returns at the guard, so a
/// direct call and the theme path cannot install the fonts twice.
pub fn install(ctx: &egui::Context) {
    let id = guard_id();
    if ctx.data(|data| data.get_temp::<bool>(id).unwrap_or(false)) {
        return;
    }
    ctx.set_fonts(definitions());
    ctx.data_mut(|data| data.insert_temp(id, true));
}

/// One fallback font face, such as a system font read from disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FallbackFont {
    /// The unique name of the face in the font definitions.
    pub name: String,
    /// The bytes of the `.ttf`, `.otf` or `.ttc` file.
    pub data: Vec<u8>,
    /// The face inside the file. Use `0` for a single-face file.
    pub index: u32,
}

/// Add fallback faces to all four families, after every face already there.
///
/// Roboto keeps its place, so a glyph that Roboto has never changes. A face
/// is used only for a char that the earlier faces lack. The faces apply at the
/// start of the next frame. A repeated name in one call is added once, because
/// egui queues two calls with one name in the same frame and both would join
/// the family lists. Call this once for each name. Two calls with one name are
/// not supported.
///
/// A face that cannot be parsed at its index is skipped, because egui panics
/// on the next frame for a bad font file. The result lists the names of the
/// skipped faces, in call order. A good face in the same call is still added.
#[must_use]
pub fn add_fallbacks(ctx: &egui::Context, fonts: Vec<FallbackFont>) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut rejected = Vec::new();
    for font in fonts {
        if !seen.insert(font.name.clone()) {
            continue;
        }
        if FontRef::from_index(&font.data, font.index).is_err() {
            rejected.push(font.name);
            continue;
        }
        let data = FontData {
            index: font.index,
            ..FontData::from_owned(font.data)
        };
        ctx.add_font(FontInsert::new(&font.name, data, fallback_families()));
    }
    rejected
}

/// The four families that receive each fallback face at the lowest priority.
fn fallback_families() -> Vec<InsertFontFamily> {
    [
        FontFamily::Proportional,
        FontFamily::Monospace,
        FontFamily::Name(MEDIUM_FAMILY.into()),
        FontFamily::Name(BOLD_FAMILY.into()),
    ]
    .into_iter()
    .map(|family| InsertFontFamily {
        family,
        priority: FontPriority::Lowest,
    })
    .collect()
}

#[cfg(test)]
mod fallback_tests;

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

    #[test]
    fn the_fonts_still_shape_after_a_direct_install_and_the_theme() {
        let ctx = egui::Context::default();
        install(&ctx);
        assert!(
            ctx.data(|data| data.get_temp::<bool>(guard_id()).unwrap_or(false)),
            "the direct install sets the guard"
        );

        crate::apply_theme(&ctx);
        assert!(
            ctx.data(|data| data.get_temp::<bool>(guard_id()).unwrap_or(false)),
            "the theme leaves the guard set"
        );

        ctx.run_ui(RawInput::default(), |_| {})
            .drop_without_applying_deltas();

        let galley = ctx.fonts_mut(|fonts| {
            fonts.layout_no_wrap(
                "Tarragon".to_owned(),
                FontId::proportional(12.0),
                Color32::WHITE,
            )
        });
        let glyphs: usize = galley.rows.iter().map(|row| row.row.glyphs.len()).sum();
        assert_eq!(glyphs, 8, "the fonts still shape text after two calls");
    }
}
