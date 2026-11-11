//! Reusable egui widgets: grid, sidebar, chips, pills, and dialogs.

use egui::{Button, RichText};
use egui::{CornerRadius, CursorIcon, FontId, Response, Sense, Stroke, StrokeKind, Ui, Vec2, vec2};

use crate::tokens;

/// A small button: `SIZE_SMALL` text with 8 by 3 point padding, as `.btn.small`.
pub(crate) fn small_button(ui: &mut Ui, text: &str) -> Response {
    ui.scope(|ui| {
        ui.spacing_mut().button_padding = vec2(8.0, 3.0);
        ui.add(Button::new(RichText::new(text).size(tokens::SIZE_SMALL)))
    })
    .inner
}

/// The height of an action button: the mockup `.btn` height.
pub(crate) const ACTION_BUTTON_HEIGHT: f32 = 30.0;

/// An action button: `SIZE_BODY` text with 12 by 6 point padding, as `.btn`.
pub(crate) fn action_button(ui: &mut Ui, text: &str) -> Response {
    ui.scope(|ui| {
        ui.spacing_mut().button_padding = vec2(tokens::SPACING_M, 6.0);
        ui.add(
            Button::new(RichText::new(text).size(tokens::SIZE_BODY))
                .min_size(vec2(0.0, ACTION_BUTTON_HEIGHT)),
        )
    })
    .inner
}

/// A ghost button: a transparent pill with a thin border, caption-size muted
/// text, and a highlight fill on hover.
pub(crate) fn ghost_button(ui: &mut Ui, text: &str) -> Response {
    let font = FontId::proportional(tokens::SIZE_CAPTION);
    let galley = ui
        .painter()
        .layout_no_wrap(text.to_owned(), font, tokens::TEXT_MUTED);
    // The stylesheet pads 2 and 8 points inside a 1 point border.
    let pad = vec2(tokens::SPACING_S, tokens::SPACING_XS / 2.0);
    let border = Vec2::splat(tokens::STROKE_WIDTH);
    let (rect, response) =
        ui.allocate_exact_size(galley.size() + (pad + border) * 2.0, Sense::click());
    let response = response.on_hover_cursor(CursorIcon::PointingHand);
    let radius = CornerRadius::same(tokens::RADIUS_XL);
    let hovered = response.hovered();
    let painter = ui.painter();
    if hovered {
        painter.rect_filled(rect, radius, tokens::SURFACE_HIGHLIGHT);
    }
    painter.rect_stroke(
        rect,
        radius,
        Stroke::new(tokens::STROKE_WIDTH, tokens::BORDER_INTERACTIVE),
        StrokeKind::Inside,
    );
    let color = if hovered {
        tokens::TEXT_SECONDARY
    } else {
        tokens::TEXT_MUTED
    };
    painter.galley(rect.min + pad + border, galley, color);
    response
}
