//! How tall the "+ add" menu is.

use crate::tokens;

/// How many tag rows the menu shows before it scrolls.
pub(super) const MENU_ROWS: f32 = 8.0;

/// The height of the line between the tag rows and "Create new...": a 1 point
/// rule with 4 points of margin either side.
pub(super) const SEPARATOR: f32 = tokens::STROKE_WIDTH + 2.0 * tokens::SPACING_XS;

/// The height of a menu with `choices` tag rows, borders and padding
/// included. The tag rows scroll past `MENU_ROWS`.
#[must_use]
#[expect(
    clippy::cast_precision_loss,
    reason = "a row count is tiny, far below 2^24"
)]
pub(super) fn menu_height(choices: usize) -> f32 {
    let tags = (choices as f32).min(MENU_ROWS) * tokens::MENU_ROW_HEIGHT;
    let separator = if choices > 0 { SEPARATOR } else { 0.0 };
    tags + separator + tokens::MENU_ROW_HEIGHT + 2.0 * (tokens::SPACING_XS + tokens::STROKE_WIDTH)
}
