//! The dashed folder icon of the empty card.

use egui::epaint::Shape;
use egui::{Painter, Pos2, Rect, Sense, Stroke, Ui, Vec2, pos2, vec2};

use crate::tokens;

/// The folder icon box, below its tab.
const ICON_SIZE: Vec2 = vec2(56.0, 44.0);
/// The tab above the folder icon.
const TAB_SIZE: Vec2 = vec2(18.0, 6.0);
/// The gap between the left edge of the icon and its tab.
const TAB_OFFSET: f32 = 8.0;
/// The length of one dash of the icon border.
const DASH: f32 = 4.0;
/// The gap between two dashes of the icon border.
const DASH_GAP: f32 = 3.0;
/// The width of the icon border.
const ICON_STROKE: f32 = 2.0;
/// The number of segments that approximate one rounded corner.
const ARC_STEPS: u8 = 4;

/// Draw the dashed folder icon: a rounded box with a tab at the top left.
pub(super) fn folder_icon(ui: &mut Ui) {
    let size = vec2(ICON_SIZE.x, ICON_SIZE.y + TAB_SIZE.y);
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let stroke = Stroke::new(ICON_STROKE, tokens::SURFACE_HIGHLIGHT);
    // Inset by half the stroke so the border stays inside the allocation.
    let inset = ICON_STROKE / 2.0;
    let body = Rect::from_min_max(
        pos2(rect.min.x + inset, rect.min.y + TAB_SIZE.y + inset),
        rect.max - vec2(inset, inset),
    );
    let mut path = rounded_path(body, f32::from(tokens::RADIUS_M), [true; 4]);
    path.push(path[0]);
    dashed(ui.painter(), &path, stroke);

    let tab = Rect::from_min_size(
        pos2(rect.min.x + TAB_OFFSET, rect.min.y + inset),
        vec2(TAB_SIZE.x, TAB_SIZE.y),
    );
    let mut tab_path = rounded_path(
        tab,
        f32::from(tokens::RADIUS_XS),
        [true, true, false, false],
    );
    // Start at the bottom left so the open path has no bottom edge.
    tab_path.rotate_right(1);
    dashed(ui.painter(), &tab_path, stroke);
}

/// Paint `path` as dashes.
fn dashed(painter: &Painter, path: &[Pos2], stroke: Stroke) {
    painter.extend(Shape::dashed_line(path, stroke, DASH, DASH_GAP));
}

/// The points of the outline of `rect`, clockwise from the top left.
///
/// `round` says which corners curve: top left, top right, bottom right, bottom
/// left. A square corner contributes its one point.
fn rounded_path(rect: Rect, radius: f32, round: [bool; 4]) -> Vec<Pos2> {
    let corners = [
        (rect.min, vec2(1.0, 1.0), 180.0_f32),
        (pos2(rect.max.x, rect.min.y), vec2(-1.0, 1.0), 270.0),
        (rect.max, vec2(-1.0, -1.0), 0.0),
        (pos2(rect.min.x, rect.max.y), vec2(1.0, -1.0), 90.0),
    ];
    let mut path = Vec::new();
    for ((corner, inward, start), curved) in corners.into_iter().zip(round) {
        if !curved {
            path.push(corner);
            continue;
        }
        let center = corner + inward * radius;
        for step in 0..=ARC_STEPS {
            let angle = (start + 90.0 * f32::from(step) / f32::from(ARC_STEPS)).to_radians();
            path.push(center + radius * vec2(angle.cos(), angle.sin()));
        }
    }
    path
}
