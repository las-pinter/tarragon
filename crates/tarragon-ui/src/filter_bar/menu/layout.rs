//! Sizes of the check menu.

use crate::tokens;

/// How many rows the menu shows before it scrolls.
pub(super) const MENU_ROWS: f32 = 12.0;

/// The space around the rule above the footer: a 1 point rule with 4 points of
/// margin either side.
pub(super) const FOOTER_SEPARATOR: f32 = tokens::STROKE_WIDTH + 2.0 * tokens::SPACING_XS;
/// What the footer adds to the menu height: the rule and one row.
pub(super) const FOOTER_HEIGHT: f32 = FOOTER_SEPARATOR + tokens::MENU_ROW_HEIGHT;

/// The height of a menu with `rows` rows, padding included. An empty menu
/// still has its one "No items" row.
#[must_use]
#[expect(
    clippy::cast_precision_loss,
    reason = "a row count is tiny, far below 2^24"
)]
pub(super) fn menu_height(rows: usize) -> f32 {
    let shown = (rows as f32).clamp(1.0, MENU_ROWS);
    shown * tokens::MENU_ROW_HEIGHT + 2.0 * tokens::MENU_PADDING
}
