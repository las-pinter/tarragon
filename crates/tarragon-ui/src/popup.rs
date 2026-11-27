//! Parts shared by the popup menus: where they open, their frame, and a row.

use egui::{
    Align2, Color32, CornerRadius, CursorIcon, FontId, Frame, Margin, Pos2, Rect, Response, Sense,
    Stroke, Ui, WidgetInfo, WidgetType, vec2,
};

use crate::theme::points;
use crate::tokens;

/// `.tick` width, in points.
pub(crate) const TICK_WIDTH: f32 = 12.0;
/// `.menu-row .lead` gap between the tick and the text, in points.
pub(crate) const LEAD_GAP: f32 = 8.0;

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

/// Draw a check mark in the tick box `tick`.
pub(crate) fn paint_tick(ui: &Ui, tick: Rect) {
    let at = |x: f32, y: f32| Pos2::new(tick.min.x + x, tick.center().y + y);
    let points = vec![at(1.0, 0.0), at(4.5, 3.5), at(11.0, -3.5)];
    ui.painter()
        .line(points, Stroke::new(1.5, tokens::AMBER_ACCENT));
}

/// The tick box at the start of `row`, after the row padding.
pub(crate) fn tick_box(row: Rect) -> Rect {
    let min = row.left_center() + vec2(tokens::MENU_ROW_PAD_X, -tokens::MENU_ROW_HEIGHT / 2.0);
    Rect::from_min_size(min, vec2(TICK_WIDTH, tokens::MENU_ROW_HEIGHT))
}

/// The rows of one menu, all as wide as the widest label.
///
/// Each row takes the coral-deep hover fill and the light coral text of the
/// other menus. With ticks, every row leaves room for the tick column, and a
/// checked row paints the tick.
pub(crate) struct Rows {
    width: f32,
    ticks: bool,
}

impl Rows {
    /// Rows for `labels`, with a tick column when `ticks` is set.
    #[must_use]
    pub(crate) fn new(ui: &Ui, labels: &[&str], ticks: bool) -> Self {
        let widest = labels
            .iter()
            .map(|label| text_width(ui, label))
            .fold(0.0, f32::max);
        let lead = if ticks { TICK_WIDTH + LEAD_GAP } else { 0.0 };
        Self {
            width: widest + lead + 2.0 * tokens::MENU_ROW_PAD_X,
            ticks,
        }
    }

    /// Draw a row with `label`, ticked when `checked`. Returns whether it was
    /// clicked.
    pub(crate) fn row(&self, ui: &mut Ui, label: &str, checked: bool) -> bool {
        let (rect, response, color) = row_frame(ui, self.width);
        let mut at = rect.left_center() + vec2(tokens::MENU_ROW_PAD_X, 0.0);
        if self.ticks {
            if checked {
                paint_tick(ui, tick_box(rect));
            }
            at.x += TICK_WIDTH + LEAD_GAP;
        }
        ui.painter().text(
            at,
            Align2::LEFT_CENTER,
            label,
            FontId::proportional(tokens::SIZE_BODY),
            color,
        );
        // Screen readers get the label and, with ticks, the checked state.
        let enabled = ui.is_enabled();
        response.widget_info(|| {
            if self.ticks {
                WidgetInfo::selected(WidgetType::Button, enabled, checked, label)
            } else {
                WidgetInfo::labeled(WidgetType::Button, enabled, label)
            }
        });
        response.clicked()
    }
}
