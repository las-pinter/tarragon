//! Sizes and the tick for the check menu.

use egui::{Pos2, Rect, Stroke, Ui, vec2};

use crate::tokens;

/// How many rows the menu shows before it scrolls.
pub(super) const MENU_ROWS: f32 = 12.0;
/// `.tick` width, in points.
pub(super) const TICK_WIDTH: f32 = 12.0;
/// `.menu-row .lead` gap between the tick and the text, in points.
pub(super) const LEAD_GAP: f32 = 8.0;

/// The space around the rule above the footer: a 1 point rule with 4 points of
/// margin either side.
pub(super) const FOOTER_SEPARATOR: f32 = tokens::STROKE_WIDTH + 2.0 * tokens::SPACING_XS;
/// What the footer adds to the menu height: the rule and one row.
pub(super) const FOOTER_HEIGHT: f32 = FOOTER_SEPARATOR + tokens::MENU_ROW_HEIGHT;

/// The height of a menu with `rows` rows, borders and padding included. An
/// empty menu still has its one "No items" row.
#[must_use]
#[expect(
    clippy::cast_precision_loss,
    reason = "a row count is tiny, far below 2^24"
)]
pub(super) fn menu_height(rows: usize) -> f32 {
    let shown = (rows as f32).clamp(1.0, MENU_ROWS);
    shown * tokens::MENU_ROW_HEIGHT + 2.0 * (tokens::SPACING_XS + tokens::STROKE_WIDTH)
}

/// Draw a check mark in the tick box `tick`.
pub(super) fn paint_tick(ui: &Ui, tick: Rect) {
    let at = |x: f32, y: f32| Pos2::new(tick.min.x + x, tick.center().y + y);
    let points = vec![at(1.0, 0.0), at(4.5, 3.5), at(11.0, -3.5)];
    ui.painter()
        .line(points, Stroke::new(1.5, tokens::AMBER_ACCENT));
}

/// The tick box at the start of `row`, after the row padding.
pub(super) fn tick_box(row: Rect) -> Rect {
    let min = row.left_center() + vec2(tokens::MENU_ROW_PAD_X, -tokens::MENU_ROW_HEIGHT / 2.0);
    Rect::from_min_size(min, vec2(TICK_WIDTH, tokens::MENU_ROW_HEIGHT))
}
