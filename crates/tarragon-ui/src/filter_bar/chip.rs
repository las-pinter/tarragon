//! A removable chip for the filter bar.

use egui::{
    Color32, CornerRadius, FontFamily, FontId, Rect, Response, Sense, Stroke, StrokeKind,
    TextStyle, Ui, pos2, vec2,
};

use crate::{fonts, tokens};

/// Horizontal chip padding, in points. The stylesheet sets 6 px.
const PAD_X: f32 = 6.0;
/// Vertical chip padding, in points. The stylesheet sets 2 px.
const PAD_Y: f32 = 2.0;

/// Draw a chip with `label` and a remove x, and return whether the x was clicked.
///
/// The gap between chips belongs to the caller row.
pub fn show_chip(ui: &mut Ui, label: &str, tooltip: &str) -> bool {
    let (response, cross) = draw(ui, label, tooltip);
    response.clicked()
        && response
            .interact_pointer_pos()
            .is_some_and(|p| cross.contains(p))
}

/// Draw the chip and return its response and the hit area of the x.
fn draw(ui: &mut Ui, label: &str, tooltip: &str) -> (Response, Rect) {
    let font = TextStyle::Small.resolve(ui.style());
    let bold = FontId::new(font.size, FontFamily::Name(fonts::BOLD_FAMILY.into()));
    let layout =
        |ui: &Ui, s: &str, font, color| ui.painter().layout_no_wrap(s.to_owned(), font, color);
    let name = layout(ui, label, font, tokens::AMBER_ACCENT);
    // The x colour depends on hover, which is known after the allocation, so
    // the galley takes the placeholder colour and the paint call sets it.
    let cross = layout(ui, "x", bold, Color32::PLACEHOLDER);
    let width = PAD_X * 2.0 + name.size().x + tokens::SPACING_XS + cross.size().x;
    let (rect, response) =
        ui.allocate_exact_size(vec2(width, name.size().y + PAD_Y * 2.0), Sense::click());
    let hit = Rect::from_min_max(
        pos2(rect.max.x - PAD_X - cross.size().x, rect.min.y),
        rect.max,
    );
    let color = if response.hovered() {
        tokens::CORAL_STRONG
    } else {
        tokens::CORAL_MUTED
    };
    let painter = ui.painter();
    let radius = CornerRadius::same(tokens::RADIUS_XL);
    let border = Stroke::new(tokens::STROKE_WIDTH, tokens::AMBER_ACCENT);
    painter.rect(
        rect,
        radius,
        tokens::SURFACE_HOVER,
        border,
        StrokeKind::Inside,
    );
    painter.galley(rect.min + vec2(PAD_X, PAD_Y), name, tokens::AMBER_ACCENT);
    painter.galley(pos2(hit.min.x, rect.min.y + PAD_Y), cross, color);
    (response.on_hover_text(tooltip), hit)
}

#[cfg(test)]
mod tests;
