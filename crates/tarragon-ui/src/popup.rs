//! Parts shared by the popup menus: where they open, their frame, and a row.

use egui::{
    Align2, Color32, CornerRadius, CursorIcon, FontId, Frame, Margin, Pos2, Rect, Response, Sense,
    Stroke, Ui, vec2,
};

use crate::theme::points;
use crate::tokens;

/// Where a menu `height` tall opens: under `button`, or above it when it does
/// not fit under and does fit above. The corner is the one that touches the
/// button side. `bounds` is the window less its edge margin.
#[must_use]
pub(crate) fn menu_placement(button: Rect, height: f32, bounds: Rect) -> (Pos2, Align2) {
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

/// The frame of a popup menu: the menu fill, a large radius, and no border.
pub(crate) fn menu_frame() -> Frame {
    Frame::new()
        .fill(tokens::SURFACE_HOVER)
        .stroke(Stroke::NONE)
        .corner_radius(CornerRadius::same(tokens::RADIUS_XL))
        .inner_margin(Margin::same(points(tokens::MENU_PADDING)))
}

/// Allocate one menu row `width` wide and paint its hover fill. Give the row
/// rect, its response, and the text color for its state.
pub(crate) fn row_frame(ui: &mut Ui, width: f32) -> (Rect, Response, Color32) {
    let (rect, response) =
        ui.allocate_exact_size(vec2(width, tokens::MENU_ROW_HEIGHT), Sense::click());
    let response = response.on_hover_cursor(CursorIcon::PointingHand);
    if !response.hovered() {
        return (rect, response, tokens::TEXT_PRIMARY);
    }
    let radius = CornerRadius::same(tokens::RADIUS_M);
    ui.painter().rect_filled(rect, radius, tokens::CORAL_DARK);
    (rect, response, tokens::CORAL_STRONG)
}

/// The width of `text` in the menu font.
pub(crate) fn text_width(ui: &Ui, text: &str) -> f32 {
    let font = FontId::proportional(tokens::SIZE_BODY);
    let galley = ui
        .painter()
        .layout_no_wrap(text.to_owned(), font, tokens::TEXT_PRIMARY);
    galley.size().x
}
