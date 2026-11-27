//! Sidebar row marks: the folder outline, the chevron caret, and the star.
//!
//! Each mark is drawn on the grid of its icon on the redesign page: 16 units
//! for the folder and the star, 12 for the caret.

use std::f32::consts::{FRAC_PI_2, PI};

use egui::epaint::{Mesh, PathShape, PathStroke};
use egui::{Color32, Painter, Pos2, Rect, Shape, Stroke, pos2, vec2};

/// The folder and star icon square, in points.
pub const ICON_SIZE: f32 = 16.0;
/// The caret square, in points.
pub const CARET_SIZE: f32 = 12.0;
/// The folder outline width on the 16 unit grid.
const FOLDER_STROKE: f32 = 1.5;
/// The caret stroke width on the 12 unit grid.
const CARET_STROKE: f32 = 1.6;
/// The corner radius of the folder outline on its grid.
const FOLDER_CORNER: f32 = 1.5;
/// Points on each rounded corner of the folder outline.
const CORNER_STEPS: usize = 4;

/// The star corners on the 16 unit grid, outer and inner by turns.
const STAR: [(f32, f32); 10] = [
    (8.0, 1.8),
    (9.9, 5.7),
    (14.2, 6.3),
    (11.1, 9.3),
    (11.8, 13.6),
    (8.0, 11.6),
    (4.2, 13.6),
    (4.9, 9.3),
    (1.8, 6.3),
    (6.1, 5.7),
];
/// A point inside the star that sees every corner, so a fan from it fills
/// the star without crossing an edge.
const STAR_CENTRE: (f32, f32) = (8.0, 8.2);

/// The points of the folder outline in the square at `origin`, on its grid.
///
/// The outline runs clockwise from the left edge: the top-left corner, the
/// tab, the step down to the body, and the three other rounded corners.
fn folder_points(origin: Pos2) -> Vec<Pos2> {
    let corner = |centre: (f32, f32), from: f32, points: &mut Vec<Pos2>| {
        for step in 0..=CORNER_STEPS {
            #[expect(
                clippy::cast_precision_loss,
                reason = "a corner has a handful of steps"
            )]
            let angle = from + FRAC_PI_2 * step as f32 / CORNER_STEPS as f32;
            points.push(
                origin
                    + vec2(
                        centre.0 + FOLDER_CORNER * angle.cos(),
                        centre.1 + FOLDER_CORNER * angle.sin(),
                    ),
            );
        }
    };
    let mut points = Vec::new();
    corner((3.5, 4.5), PI, &mut points);
    points.push(origin + vec2(6.0, 3.0));
    points.push(origin + vec2(7.5, 4.5));
    corner((12.5, 6.0), -FRAC_PI_2, &mut points);
    corner((12.5, 11.5), 0.0, &mut points);
    corner((3.5, 11.5), FRAC_PI_2, &mut points);
    points
}

/// The three points of a chevron caret in the square at `origin`.
///
/// It points right when closed and down when open: the open caret is the
/// closed one turned a quarter turn about the centre of its square.
fn caret_points(origin: Pos2, expanded: bool) -> [Pos2; 3] {
    let closed = [(4.5, 3.0), (7.5, 6.0), (4.5, 9.0)];
    let half = CARET_SIZE / 2.0;
    closed.map(|(x, y)| {
        let (dx, dy) = (x - half, y - half);
        let (x, y) = if expanded {
            (half - dy, half + dx)
        } else {
            (x, y)
        };
        origin + vec2(x, y)
    })
}

/// Paint a folder outline in the icon square at `origin`.
pub fn paint_folder_icon(painter: &Painter, origin: Pos2, color: Color32) {
    painter.add(Shape::Path(PathShape::closed_line(
        folder_points(origin),
        PathStroke::new(FOLDER_STROKE, color),
    )));
}

/// Paint a chevron caret in the caret square at `origin`.
pub fn paint_caret(painter: &Painter, origin: Pos2, expanded: bool, color: Color32) {
    painter.add(Shape::line(
        caret_points(origin, expanded).to_vec(),
        Stroke::new(CARET_STROKE, color),
    ));
}

/// Paint a filled star in the icon square at `origin`.
///
/// The star is not convex, so it is filled as a fan of triangles from a point
/// that sees every corner.
pub fn paint_star(painter: &Painter, origin: Pos2, color: Color32) {
    let at = |(x, y): (f32, f32)| origin + vec2(x, y);
    let mut mesh = Mesh::default();
    mesh.colored_vertex(at(STAR_CENTRE), color);
    for corner in STAR {
        mesh.colored_vertex(at(corner), color);
    }
    #[expect(clippy::cast_possible_truncation, reason = "the star has ten corners")]
    let count = STAR.len() as u32;
    for index in 0..count {
        mesh.add_triangle(0, 1 + index, 1 + (index + 1) % count);
    }
    painter.add(Shape::mesh(mesh));
}

/// The icon square centred on the row `rect` at `x`, snapped to whole points.
#[must_use]
pub fn icon_origin(rect: Rect, x: f32, size: f32) -> Pos2 {
    pos2(x, (rect.center().y - size / 2.0).round())
}

#[cfg(test)]
mod tests;
