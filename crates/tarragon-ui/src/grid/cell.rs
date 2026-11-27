//! Painting and hit testing for one gallery tile.
//!
//! The tile follows the gallery of design/redesign/main-window.html and the
//! thumbnail states of design/redesign/components.html: a square thumbnail
//! with a file-type badge, the name below it, and an outline outside the
//! square for hover and selection. Nothing scales, fades, or casts a shadow.

use std::path::Path;
use std::sync::Arc;

use egui::text::{LayoutJob, TextFormat, TextWrapping};
use egui::{
    Align2, Color32, CornerRadius, FontFamily, FontId, Id, Painter, Rect, Response, Sense, Stroke,
    StrokeKind, Ui, Vec2, epaint, pos2, vec2,
};

use crate::grid::{GridItem, Selection};
use crate::preview::size_text;
use crate::{fonts, tokens};

/// Gap between the thumbnail and the file name, in points.
const NAME_GAP: f32 = 8.0;
/// The height kept for the one-line name, in points.
const NAME_HEIGHT: f32 = 16.0;
/// The name sits this far in from the thumbnail edge, in points.
const NAME_PAD: f32 = 2.0;
/// The room under the thumbnail: the gap and the name.
pub(crate) const LABEL_BLOCK: f32 = NAME_GAP + NAME_HEIGHT;
/// The outline width, in points.
const OUTLINE_WIDTH: f32 = 2.0;
/// The gap between the thumbnail and its outline, in points.
const OUTLINE_GAP: f32 = 3.0;
/// How far the outline reaches outside the thumbnail, in points.
pub(crate) const OUTLINE_REACH: f32 = OUTLINE_GAP + OUTLINE_WIDTH;
/// The badge and check inset from the thumbnail corner, in points.
const CORNER_INSET: f32 = 8.0;
/// Badge text size, in points.
const BADGE_SIZE: f32 = 10.0;
/// Badge letter spacing, in points: 0.04 em of the badge text.
const BADGE_SPACING: f32 = 0.04 * BADGE_SIZE;
/// Badge side padding, in points.
const BADGE_PAD_X: f32 = 6.0;
/// Badge top and bottom padding, in points.
const BADGE_PAD_Y: f32 = 3.0;
/// The padding of the hover info bar, across and down, in points.
const INFO_PAD: Vec2 = vec2(8.0, 6.0);
/// Error mark diameter, in points. The mockup ring is 44 px.
pub(crate) const ERROR_MARK_SIZE: f32 = 44.0;
/// Error mark text size, in points.
const ERROR_MARK_FONT: f32 = 24.0;
/// Check badge diameter, in points.
const CHECK_SIZE: f32 = 22.0;
/// The check mark is drawn on a 12 unit square.
const CHECK_MARK: f32 = 12.0;
/// The stroke of the check mark, in points.
const CHECK_STROKE: f32 = 2.0;

/// The visual state of one tile for this frame.
#[derive(Clone, Copy, Debug)]
struct TileState {
    /// Whether the pointer is over the tile.
    hovered: bool,
    /// Whether the tile is selected.
    selected: bool,
    /// Whether more than one tile is selected.
    multi: bool,
}

/// The widget id of the tile for item `index`.
pub(crate) fn tile_id(index: usize) -> Id {
    Id::new(("tarragon-grid-tile", index))
}

/// Paint one tile and return its interaction response.
///
/// The tile id comes from `index`, so the id stays stable across frames.
pub(crate) fn show_cell(
    ui: &mut Ui,
    rect: Rect,
    item: &GridItem,
    index: usize,
    selection: &Selection,
) -> Response {
    let response = ui.interact(rect, tile_id(index), Sense::click());
    let state = TileState {
        hovered: response.hovered(),
        selected: selection.contains(index),
        multi: selection.len() > 1,
    };
    paint(ui.painter(), rect, item, state);
    response
}

/// Paint the thumbnail, its outline, the badge, the info bar, the check, and
/// the name.
fn paint(painter: &Painter, rect: Rect, item: &GridItem, state: TileState) {
    let radius = CornerRadius::same(tokens::RADIUS_L);
    let thumb = thumbnail_rect(rect);
    paint_thumbnail(painter, thumb, item, radius);
    paint_badge(
        painter,
        thumb,
        &extension_label(&item.path),
        badge_color(&item.path),
    );
    if state.hovered {
        paint_info(painter, thumb, item);
    }
    if state.selected && state.multi {
        paint_check(painter, thumb);
    }
    if let Some(color) = outline_color(state) {
        paint_outline(painter, thumb, color);
    }
    paint_name(painter, thumb, item, state.selected);
}

/// The outline colour: coral for a selected tile, the raised step for a
/// hovered one, and none at rest. Selection wins.
fn outline_color(state: TileState) -> Option<Color32> {
    if state.selected {
        Some(tokens::CORAL_BRIGHT)
    } else if state.hovered {
        Some(tokens::SURFACE_HIGHLIGHT)
    } else {
        None
    }
}

/// The square thumbnail at the top of the cell, as wide as the cell.
fn thumbnail_rect(rect: Rect) -> Rect {
    Rect::from_min_size(rect.min, Vec2::splat(rect.width()))
}

/// Draw the outline `OUTLINE_GAP` outside the thumbnail. Its corners follow
/// the thumbnail's, grown by the same distance.
fn paint_outline(painter: &Painter, thumb: Rect, color: Color32) {
    let reach = OUTLINE_GAP + OUTLINE_WIDTH / 2.0;
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "the radius is a few points"
    )]
    let radius = (f32::from(tokens::RADIUS_L) + reach).round() as u8;
    painter.rect_stroke(
        thumb.expand(reach),
        CornerRadius::same(radius),
        Stroke::new(OUTLINE_WIDTH, color),
        StrokeKind::Middle,
    );
}

/// The largest rect with the texture's aspect ratio that fits inside `bounds`,
/// centered. The picture is letterboxed, never cropped or stretched.
fn fit_rect(bounds: Rect, texture: Vec2) -> Rect {
    if texture.x <= 0.0 || texture.y <= 0.0 {
        return bounds;
    }
    let scale = (bounds.width() / texture.x).min(bounds.height() / texture.y);
    Rect::from_center_size(bounds.center(), texture * scale)
}

/// Paint the square on the field fill and the texture fitted inside it, or
/// the placeholder with the extension text.
///
/// A failed item without a texture gets the error placeholder instead.
fn paint_thumbnail(painter: &Painter, rect: Rect, item: &GridItem, radius: CornerRadius) {
    let Some(texture) = &item.texture else {
        if item.error.is_some() {
            paint_error(painter, rect, radius);
            return;
        }
        painter.rect_filled(rect, radius, tokens::BG_TERTIARY);
        painter.text(
            rect.center(),
            Align2::CENTER_CENTER,
            extension_label(&item.path),
            FontId::proportional(tokens::SIZE_CAPTION),
            tokens::TEXT_TERTIARY,
        );
        return;
    };

    painter.rect_filled(rect, radius, tokens::BG_TERTIARY);
    let fitted = fit_rect(rect, texture.size_vec2());
    let shape = epaint::RectShape {
        rect: fitted,
        corner_radius: touching_corners(fitted, rect, radius),
        fill: Color32::WHITE,
        stroke: Stroke::NONE,
        stroke_kind: StrokeKind::Inside,
        round_to_pixels: None,
        blur_width: 0.0,
        brush: Some(Arc::new(epaint::Brush {
            fill_texture_id: texture.id(),
            uv: Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
        })),
        angle: 0.0,
    };
    painter.add(shape);
}

/// The corners of `inner` that lie on a corner of `outer` keep `radius`; the
/// rest are square. A letterboxed picture then meets the rounded thumbnail
/// without rounded corners floating inside it.
fn touching_corners(inner: Rect, outer: Rect, radius: CornerRadius) -> CornerRadius {
    let near = |a: f32, b: f32| (a - b).abs() < 0.5;
    let (left, right) = (
        near(inner.min.x, outer.min.x),
        near(inner.max.x, outer.max.x),
    );
    let (top, bottom) = (
        near(inner.min.y, outer.min.y),
        near(inner.max.y, outer.max.y),
    );
    let keep = |on: bool, value: u8| if on { value } else { 0 };
    CornerRadius {
        nw: keep(top && left, radius.nw),
        ne: keep(top && right, radius.ne),
        sw: keep(bottom && left, radius.sw),
        se: keep(bottom && right, radius.se),
    }
}

/// Paint the error placeholder: a coral ring with a bang, as in the error
/// state mockup (`design/mockups/error-state.html`, `.err-icon`).
pub(crate) fn paint_error(painter: &Painter, rect: Rect, radius: CornerRadius) {
    painter.rect(
        rect,
        radius,
        tokens::BG_SECONDARY,
        Stroke::NONE,
        StrokeKind::Inside,
    );
    painter.circle_stroke(
        rect.center(),
        // The mockup ring is 44 px border-box, so the stroke centre sits 1 px in.
        ERROR_MARK_SIZE / 2.0 - 1.0,
        Stroke::new(2.0, tokens::CORAL_STRONG),
    );
    painter.text(
        rect.center(),
        Align2::CENTER_CENTER,
        "!",
        FontId::new(ERROR_MARK_FONT, FontFamily::Name(fonts::BOLD_FAMILY.into())),
        tokens::CORAL_STRONG,
    );
}

/// Paint the file-type badge at the thumbnail top left: the format colour on
/// the solid base fill, in the medium face with a little letter spacing.
fn paint_badge(painter: &Painter, thumb: Rect, badge: &str, text_color: Color32) {
    let mut job = LayoutJob::default();
    job.append(
        badge,
        0.0,
        TextFormat {
            font_id: FontId::new(BADGE_SIZE, FontFamily::Name(fonts::MEDIUM_FAMILY.into())),
            extra_letter_spacing: BADGE_SPACING,
            color: text_color,
            ..TextFormat::default()
        },
    );
    let galley = painter.layout_job(job);
    let min = thumb.min + Vec2::splat(CORNER_INSET);
    let size = galley.size() + vec2(2.0 * BADGE_PAD_X, 2.0 * BADGE_PAD_Y);
    let rect = Rect::from_min_size(min, size);
    painter.rect_filled(
        rect,
        CornerRadius::same(tokens::RADIUS_S),
        tokens::BG_PRIMARY,
    );
    painter.galley(
        rect.min + vec2(BADGE_PAD_X, BADGE_PAD_Y),
        galley,
        text_color,
    );
}

/// The badge text colour for a file extension.
///
/// The colours follow the badges of design/redesign/main-window.html: PNG
/// navy, JPG sage, PSD coral, WEBP teal. The other formats keep the text
/// colours of `design/mockups/busy-states.html`: GIF and KRA plum, TIFF
/// umber, BMP olive, CLIP azure, and anything else the neutral grey.
fn badge_color(path: &Path) -> Color32 {
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    match extension.as_str() {
        "jpg" | "jpeg" => tokens::BADGE_FG_SAGE,
        "png" => tokens::BADGE_FG_NAVY,
        "psd" | "psb" => tokens::BADGE_FG_VINOUS,
        "webp" => tokens::BADGE_FG_TEAL,
        "gif" | "kra" => tokens::BADGE_FG_PLUM,
        "tif" | "tiff" => tokens::BADGE_FG_UMBER,
        "bmp" => tokens::BADGE_FG_OLIVE,
        "clip" => tokens::BADGE_FG_AZURE,
        _ => tokens::BADGE_FG_NEUTRAL,
    }
}

/// The hover bar text: "2048 × 1536 · 2.4 MB", or the size alone while the
/// pixel size is unknown.
pub(crate) fn info_text(item: &GridItem) -> String {
    let size = size_text(item.size);
    match item.dimensions {
        Some((width, height)) => format!("{width} × {height} · {size}"),
        None => size,
    }
}

/// Paint the hover info bar along the bottom of the thumbnail, on the base
/// fill, with the thumbnail's bottom corners.
fn paint_info(painter: &Painter, thumb: Rect, item: &GridItem) {
    let mut job = LayoutJob::simple_singleline(
        info_text(item),
        FontId::proportional(tokens::SIZE_CAPTION),
        tokens::TEXT_TERTIARY,
    );
    job.wrap = TextWrapping {
        max_width: (thumb.width() - 2.0 * INFO_PAD.x).max(0.0),
        max_rows: 1,
        break_anywhere: true,
        overflow_character: Some('…'),
    };
    let galley = painter.layout_job(job);
    let height = galley.size().y + 2.0 * INFO_PAD.y;
    let bar = Rect::from_min_max(pos2(thumb.min.x, thumb.max.y - height), thumb.max);
    let corners = CornerRadius {
        sw: tokens::RADIUS_L,
        se: tokens::RADIUS_L,
        ..CornerRadius::ZERO
    };
    painter.rect_filled(bar, corners, tokens::BG_PRIMARY);
    painter.galley(bar.min + INFO_PAD, galley, tokens::TEXT_TERTIARY);
}

/// Paint the file name under the thumbnail, left-aligned on one line. A
/// selected name takes the light coral and the medium face; a failed tile
/// names its file in coral, as the error mockup does.
fn paint_name(painter: &Painter, thumb: Rect, item: &GridItem, selected: bool) {
    let Some(name) = item.path.file_name().and_then(|name| name.to_str()) else {
        return;
    };
    let failed = item.error.is_some() && item.texture.is_none();
    let color = if selected || failed {
        tokens::CORAL_STRONG
    } else {
        tokens::TEXT_TERTIARY
    };
    let family = if selected {
        FontFamily::Name(fonts::MEDIUM_FAMILY.into())
    } else {
        FontFamily::Proportional
    };
    let mut job = LayoutJob::simple_singleline(
        name.to_owned(),
        FontId::new(tokens::SIZE_BODY, family),
        color,
    );
    job.wrap = TextWrapping {
        max_width: (thumb.width() - 2.0 * NAME_PAD).max(0.0),
        max_rows: 1,
        break_anywhere: true,
        overflow_character: Some('…'),
    };
    let galley = painter.layout_job(job);
    painter.galley(
        pos2(thumb.min.x + NAME_PAD, thumb.max.y + NAME_GAP),
        galley,
        color,
    );
}

/// Paint the coral check badge at the thumbnail top right: a dark check on
/// the coral circle.
fn paint_check(painter: &Painter, thumb: Rect) {
    let half = CHECK_SIZE / 2.0;
    let center = pos2(
        thumb.max.x - CORNER_INSET - half,
        thumb.min.y + CORNER_INSET + half,
    );
    painter.circle_filled(center, half, tokens::CORAL_BRIGHT);
    // The page draws the check from (2.5, 6.2) to (5, 8.5) to (9.5, 3.5).
    let origin = center - Vec2::splat(CHECK_MARK / 2.0);
    let at = |x: f32, y: f32| origin + vec2(x, y);
    painter.add(epaint::PathShape::line(
        vec![at(2.5, 6.2), at(5.0, 8.5), at(9.5, 3.5)],
        Stroke::new(CHECK_STROKE, tokens::BG_PRIMARY),
    ));
}

/// The upper-case file extension, or `FILE` when the path has none.
///
/// The badge and the placeholder share this text, so the two cannot disagree.
fn extension_label(path: &Path) -> String {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map_or_else(|| "FILE".to_owned(), str::to_ascii_uppercase)
}

#[cfg(test)]
mod state_tests;
#[cfg(test)]
mod tests;
