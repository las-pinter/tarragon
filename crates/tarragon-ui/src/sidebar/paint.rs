//! Sidebar row marks: the folder icon and the open/closed caret.
//!
//! The sizes follow design/mockups/theme.css.

use egui::epaint::PathShape;
use egui::{Color32, CornerRadius, Painter, Pos2, Rect, Shape, pos2, vec2};

/// Folder body width, in points.
pub const ICON_WIDTH: f32 = 13.0;
/// Folder body height, in points.
pub const ICON_HEIGHT: f32 = 10.0;
/// Folder tab width, in points.
const TAB_WIDTH: f32 = 6.0;
/// How far the folder tab paints above the icon origin, in points.
pub const ICON_OVERHANG: f32 = 3.0;
/// Caret length along its pointing direction, in points.
const CARET_DEPTH: f32 = 4.0;
/// Caret width across its pointing direction, in points.
const CARET_SPAN: f32 = 6.0;

/// The body and tab rects of a folder icon whose body starts at `origin`.
fn icon_rects(origin: Pos2) -> (Rect, Rect) {
    let body = Rect::from_min_size(origin, vec2(ICON_WIDTH, ICON_HEIGHT));
    let tab = Rect::from_min_size(
        pos2(origin.x, origin.y - ICON_OVERHANG),
        vec2(TAB_WIDTH, ICON_OVERHANG),
    );
    (body, tab)
}

/// The triangle corners of a caret centered in `rect`.
///
/// It points down when open and right when closed.
fn caret_points(rect: Rect, expanded: bool) -> [Pos2; 3] {
    let c = rect.center().round();
    let (w, h) = (CARET_SPAN / 2.0, CARET_DEPTH / 2.0);
    if expanded {
        [
            pos2(c.x - w, c.y - h),
            pos2(c.x + w, c.y - h),
            pos2(c.x, c.y + h),
        ]
    } else {
        [
            pos2(c.x - h, c.y - w),
            pos2(c.x + h, c.y),
            pos2(c.x - h, c.y + w),
        ]
    }
}

/// Paint a folder icon whose body starts at `origin`.
///
/// The tab paints `ICON_OVERHANG` above `origin`, outside the body box.
pub fn paint_folder_icon(painter: &Painter, origin: Pos2, color: Color32) {
    let (body, tab) = icon_rects(origin);
    painter.rect_filled(body, CornerRadius::same(2), color);
    painter.rect_filled(
        tab,
        CornerRadius {
            nw: 1,
            ne: 1,
            sw: 0,
            se: 0,
        },
        color,
    );
}

/// Paint a caret centred in `rect`.
pub fn paint_caret(painter: &Painter, rect: Rect, expanded: bool, color: Color32) {
    let points = caret_points(rect, expanded).to_vec();
    painter.add(Shape::Path(PathShape::convex_polygon(
        points,
        color,
        egui::Stroke::NONE,
    )));
}

#[cfg(test)]
mod tests;
