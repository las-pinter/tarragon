//! The details of one previewed file, and the grid that shows them.

use egui::{Grid, RichText, Ui, vec2};

use crate::tokens;

/// The text for a value that is not known.
const UNKNOWN: &str = "Unknown";

/// The unit names, each 1024 times the one before.
const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];

/// The details of one file, as the Metadata block shows them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Meta {
    /// The file name, without the folder.
    pub name: String,
    /// The pixel width and height of the original, when known.
    pub dimensions: Option<(u32, u32)>,
    /// The file size in bytes, when known.
    pub size: Option<u64>,
    /// The format name, such as "PNG".
    pub format: String,
}

impl Meta {
    /// The label and value text of each row, in the order they are drawn.
    #[must_use]
    pub fn rows(&self) -> [(&'static str, String); 4] {
        [
            ("File", self.name.clone()),
            (
                "Dimensions",
                self.dimensions.map_or_else(unknown, dimensions_text),
            ),
            ("Size", self.size.map_or_else(unknown, size_text)),
            ("Format", self.format.clone()),
        ]
    }
}

/// The text for a missing value, for `map_or_else`.
fn unknown() -> String {
    UNKNOWN.to_owned()
}

/// Width and height as "2048 x 1536".
#[must_use]
pub fn dimensions_text((width, height): (u32, u32)) -> String {
    format!("{width} x {height}")
}

/// A byte count with one decimal and the largest unit that keeps the number
/// below 1024, such as "2.4 MB". Whole bytes show without a decimal.
#[must_use]
pub fn size_text(bytes: u64) -> String {
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let mut unit = 1;
    // Whole tenths, rounded half up. A value that rounds up to 1024 moves to
    // the next unit, so the text never reads "1024.0 KB".
    let tenths = |unit: usize| (u128::from(bytes) * 10 + (1u128 << (10 * unit - 1))) >> (10 * unit);
    while unit < UNITS.len() - 1 && tenths(unit) >= 10240 {
        unit += 1;
    }
    let tenths = tenths(unit);
    format!("{}.{} {}", tenths / 10, tenths % 10, UNITS[unit])
}

/// Draw the Metadata heading and the label and value grid.
pub fn show_grid(ui: &mut Ui, meta: &Meta) {
    ui.label(
        RichText::new("Metadata")
            .size(tokens::SIZE_CAPTION)
            .color(tokens::TEXT_MUTED),
    );
    Grid::new("preview-meta")
        .spacing(vec2(tokens::SPACING_S, tokens::SPACING_XS / 2.0))
        .show(ui, |ui| {
            for (key, value) in meta.rows() {
                ui.label(
                    RichText::new(key)
                        .size(tokens::SIZE_CAPTION)
                        .color(tokens::TEXT_MUTED),
                );
                ui.label(
                    RichText::new(value)
                        .size(tokens::SIZE_CAPTION)
                        .color(tokens::TEXT_TERTIARY),
                );
                ui.end_row();
            }
        });
}

#[cfg(test)]
mod tests;
