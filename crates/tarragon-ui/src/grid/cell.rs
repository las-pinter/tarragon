//! Painting and hit testing for one gallery tile.

use std::path::Path;
use std::sync::Arc;

use egui::text::{LayoutJob, TextWrapping};
use egui::{
    Align2, Color32, CornerRadius, FontId, Id, Painter, Rect, Response, Sense, Stroke, StrokeKind,
    Ui, epaint, pos2, vec2,
};

use crate::grid::{GridItem, Selection};
use crate::tokens;

/// Gap between the thumbnail and the file name, in points.
const NAME_GAP: f32 = 6.0;
/// Badge text size, in points.
const BADGE_SIZE: f32 = 9.0;
/// Badge side padding, in points.
const BADGE_PAD_X: f32 = 5.0;
/// Badge top and bottom padding, in points.
const BADGE_PAD_Y: f32 = 1.0;
/// Check badge diameter, in points.
const CHECK_SIZE: f32 = 18.0;
/// Check badge inset from the cell corner, in points.
const CHECK_INSET: f32 = 6.0;

/// The visual state of one tile for this frame.
#[derive(Clone, Copy, Debug)]
struct TileState {
    /// Hover fade, from zero to one.
    hover: f32,
    /// Whether the tile is selected.
    selected: bool,
    /// Whether more than one tile is selected.
    multi: bool,
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
    let id = Id::new(("tarragon-grid-tile", index));
    let response = ui.interact(rect, id, Sense::click());
    let hover = ui.ctx().animate_bool_with_time(
        id.with("hover"),
        response.hovered(),
        tokens::DURATION_FAST,
    );
    let state = TileState {
        hover,
        selected: selection.contains(index),
        multi: selection.len() > 1,
    };
    paint(ui.painter(), rect, item, state);
    response
}

/// Paint the cell box, the thumbnail, the badge, the name, and the check.
fn paint(painter: &Painter, rect: Rect, item: &GridItem, state: TileState) {
    let radius = CornerRadius::same(tokens::RADIUS_L);
    // The stylesheet scales a hovered cell to 1.02. The cell keeps its size.
    let fill = if state.selected {
        tokens::BG_SECONDARY
    } else {
        tokens::BG_PRIMARY.lerp_to_gamma(tokens::BG_SECONDARY, state.hover)
    };
    let (border, width) = if state.selected {
        (tokens::CORAL_MUTED, 1.5)
    } else {
        (
            tokens::BORDER_SUBTLE.lerp_to_gamma(tokens::BORDER_CARD, state.hover),
            1.0,
        )
    };

    if state.hover > 0.0 {
        painter.add(lift_shadow(rect, radius, state.hover));
    }
    painter.rect(
        rect,
        radius,
        fill,
        Stroke::new(width, border),
        StrokeKind::Inside,
    );

    let thumb = thumbnail_rect(rect);
    paint_thumbnail(painter, thumb, item, radius);
    if state.selected {
        painter.rect_stroke(
            thumb,
            radius,
            Stroke::new(1.0, tokens::CORAL_MUTED),
            StrokeKind::Inside,
        );
    }
    paint_badge(
        painter,
        thumb,
        &extension_label(&item.path),
        badge_colors(&item.path),
    );
    paint_name(painter, rect, thumb, item);
    if state.selected && state.multi {
        paint_check(painter, rect);
    }
}

/// The hover lift. The shadow fades with the hover amount.
fn lift_shadow(rect: Rect, radius: CornerRadius, hover: f32) -> epaint::Shape {
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "the hover amount stays in 0..=1, so the alpha stays in 0..=127"
    )]
    let alpha = (127.0 * hover) as u8;
    let shadow = epaint::Shadow {
        offset: [0, 8],
        blur: 20,
        spread: 0,
        color: Color32::from_black_alpha(alpha),
    };
    shadow.as_shape(rect, radius).into()
}

/// The thumbnail area: the cell inset by the grid gap and the cell border,
/// one thumbnail tall.
fn thumbnail_rect(rect: Rect) -> Rect {
    // The 1 pt cell border sits outside the gap, so the thumbnail starts 15 pt in.
    let inset = tokens::GRID_GAP + 1.0;
    Rect::from_min_size(
        rect.min + vec2(inset, inset),
        vec2(
            (rect.width() - 2.0 * inset).max(0.0),
            tokens::THUMBNAIL_SIZE,
        ),
    )
}

/// Paint the uploaded texture, or the placeholder with the extension text.
fn paint_thumbnail(painter: &Painter, rect: Rect, item: &GridItem, radius: CornerRadius) {
    let Some(texture) = &item.texture else {
        painter.rect(
            rect,
            radius,
            tokens::BG_TERTIARY,
            Stroke::NONE,
            StrokeKind::Inside,
        );
        painter.text(
            rect.center(),
            Align2::CENTER_CENTER,
            extension_label(&item.path),
            FontId::proportional(tokens::SIZE_CAPTION),
            tokens::TEXT_TERTIARY,
        );
        return;
    };

    let shape = epaint::RectShape {
        rect,
        corner_radius: radius,
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

/// Paint the file-type badge at the thumbnail top left.
fn paint_badge(
    painter: &Painter,
    thumb: Rect,
    badge: &str,
    (fill, text_color): (Color32, Color32),
) {
    let galley = painter.layout_no_wrap(
        badge.to_owned(),
        FontId::proportional(BADGE_SIZE),
        text_color,
    );
    let min = thumb.min + vec2(tokens::SPACING_XS, tokens::SPACING_XS);
    let size = galley.size() + vec2(2.0 * BADGE_PAD_X, 2.0 * BADGE_PAD_Y);
    let rect = Rect::from_min_size(min, size);
    painter.rect(
        rect,
        CornerRadius::same(tokens::RADIUS_S),
        fill,
        Stroke::NONE,
        StrokeKind::Inside,
    );
    painter.galley(
        rect.min + vec2(BADGE_PAD_X, BADGE_PAD_Y),
        galley,
        text_color,
    );
}

/// The badge fill and text colours for a file extension.
///
/// The pairs follow `design/mockups/busy-states.html` lines 55-62: JPG sage,
/// PNG navy, PSD vinous, WEBP teal, GIF plum, TIFF umber, BMP olive, KRA plum.
/// CLIP takes azure, and an unknown or absent extension takes the neutral pair.
fn badge_colors(path: &Path) -> (Color32, Color32) {
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    match extension.as_str() {
        "jpg" | "jpeg" => (tokens::BADGE_BG_SAGE, tokens::BADGE_FG_SAGE),
        "png" => (tokens::BADGE_BG_NAVY, tokens::BADGE_FG_NAVY),
        "psd" | "psb" => (tokens::BADGE_BG_VINOUS, tokens::BADGE_FG_VINOUS),
        "webp" => (tokens::BADGE_BG_TEAL, tokens::BADGE_FG_TEAL),
        "gif" | "kra" => (tokens::BADGE_BG_PLUM, tokens::BADGE_FG_PLUM),
        "tif" | "tiff" => (tokens::BADGE_BG_UMBER, tokens::BADGE_FG_UMBER),
        "bmp" => (tokens::BADGE_BG_OLIVE, tokens::BADGE_FG_OLIVE),
        "clip" => (tokens::BADGE_BG_AZURE, tokens::BADGE_FG_AZURE),
        _ => (tokens::BADGE_BG_NEUTRAL, tokens::BADGE_FG_NEUTRAL),
    }
}

/// Paint the file name under the thumbnail, centered and on one line.
fn paint_name(painter: &Painter, rect: Rect, thumb: Rect, item: &GridItem) {
    let Some(name) = item.path.file_name().and_then(|name| name.to_str()) else {
        return;
    };
    let mut job = LayoutJob::simple_singleline(
        name.to_owned(),
        FontId::proportional(tokens::SIZE_SMALL),
        tokens::TEXT_MUTED,
    );
    job.wrap = TextWrapping {
        max_width: (rect.width() - 2.0 * tokens::GRID_GAP).max(0.0),
        max_rows: 1,
        break_anywhere: true,
        overflow_character: Some('…'),
    };
    let galley = painter.layout_job(job);
    let left = rect.center().x - galley.size().x / 2.0;
    painter.galley(
        pos2(left, thumb.max.y + NAME_GAP),
        galley,
        tokens::TEXT_MUTED,
    );
}

/// Paint the coral check badge at the cell top right.
fn paint_check(painter: &Painter, rect: Rect) {
    let half = CHECK_SIZE / 2.0;
    let center = pos2(
        rect.max.x - CHECK_INSET - half,
        rect.min.y + CHECK_INSET + half,
    );
    painter.circle_filled(center, half, tokens::CORAL_MUTED);
    let stroke = Stroke::new(1.5, tokens::TEXT_PRIMARY);
    let corner = center + vec2(-1.5, 2.0);
    painter.line_segment([center + vec2(-4.5, -1.5), corner], stroke);
    painter.line_segment([corner, center + vec2(4.0, -3.0)], stroke);
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
mod tests;
