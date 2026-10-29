//! Where the "+ add" menu opens and how tall it is.

use egui::{Align2, Pos2, Rect, vec2};

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

/// Where a menu `height` tall opens: under `button`, or above it when it does
/// not fit under and does fit above. The corner is the one that touches the
/// button side. `bounds` is the window less its edge margin.
#[must_use]
pub(super) fn menu_placement(button: Rect, height: f32, bounds: Rect) -> (Pos2, Align2) {
    let gap = tokens::SPACING_XS;
    let below = button.left_bottom() + vec2(0.0, gap);
    let fits_below = below.y + height <= bounds.bottom();
    let fits_above = button.top() - gap - height >= bounds.top();
    if fits_below || !fits_above {
        (below, Align2::LEFT_TOP)
    } else {
        (button.left_top() - vec2(0.0, gap), Align2::LEFT_BOTTOM)
    }
}
