//! The picture box at the top of the Preview panel.

use egui::{Color32, CornerRadius, Pos2, Rect, Sense, TextureId, Ui, Vec2, pos2, vec2};

use crate::tokens;

/// A picture that is already on the GPU.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PreviewImage {
    /// The texture that holds the pixels.
    pub texture: TextureId,
    /// The pixel width and height of the texture.
    pub size: (u32, u32),
}

/// Where a picture of `image` pixels is drawn inside `area`: centred, with the
/// same aspect ratio, scaled up or down until one side touches the area.
///
/// A zero size on either side gives an empty rect at the centre of `area`.
#[must_use]
#[expect(
    clippy::cast_precision_loss,
    reason = "a side above 2^24 pixels rounds slightly, which is invisible"
)]
pub fn fit_rect(image: (u32, u32), area: Rect) -> Rect {
    let size = vec2(image.0 as f32, image.1 as f32);
    if size.x <= 0.0 || size.y <= 0.0 || area.width() <= 0.0 || area.height() <= 0.0 {
        return Rect::from_center_size(area.center(), Vec2::ZERO);
    }
    let scale = (area.width() / size.x).min(area.height() / size.y);
    Rect::from_center_size(area.center(), size * scale)
}

/// The room, in points, that the box leaves free at the bottom.
const SLACK: f32 = 0.5;

/// The box height that fills a `viewport` when `below` of content follows the
/// box, including the gap. A short viewport keeps the minimum and scrolls.
///
/// The box stops half a point short of the viewport. Content that ends exactly
/// at the edge could flip the scroll bar on and off through float rounding.
#[must_use]
pub fn box_height(viewport: f32, below: f32) -> f32 {
    (viewport - below - SLACK).max(tokens::PREVIEW_IMAGE_HEIGHT)
}

/// Draw the dark box, `height` tall, and the picture inside it. Returns the
/// box rect.
pub fn show_box(ui: &mut Ui, image: &PreviewImage, height: f32) -> Rect {
    let size = vec2(ui.available_width(), height);
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(
        rect,
        CornerRadius::same(tokens::RADIUS_L),
        tokens::BG_SECONDARY,
    );
    let uv = Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0));
    painter.image(
        image.texture,
        fit_rect(image.size, rect),
        uv,
        Color32::WHITE,
    );
    rect
}

#[cfg(test)]
mod tests;
