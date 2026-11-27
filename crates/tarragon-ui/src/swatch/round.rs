//! A round color swatch: a filled circle, or a checkerboard cut to a circle.
//!
//! The filter row of design/redesign/main-window.html draws its swatches as
//! circles, and the checkerboard swatch as 4 by 4 cells with the top-left
//! cell dark.

use egui::{Painter, Pos2, Rect, Shape, Stroke, pos2, vec2};

use super::{Swatch, bucket_color};
use crate::tokens;

/// Points on the circle outline that the checker cells are cut from.
const OUTLINE_POINTS: usize = 48;
/// Checker cells along one side of the circle's bounding square.
const CHECKER_SPLIT: u8 = 4;

/// Paint `swatch` as a circle of `radius` around `center`.
pub fn paint_round(painter: &Painter, center: Pos2, radius: f32, swatch: Swatch) {
    match swatch {
        Swatch::Bucket(bucket) => {
            painter.circle_filled(center, radius, bucket_color(bucket));
        }
        Swatch::Transparent => {
            painter.circle_filled(center, radius, tokens::CHECKER_LIGHT);
            for cell in dark_cells(center, radius) {
                painter.add(Shape::convex_polygon(
                    cell,
                    tokens::CHECKER_DARK,
                    Stroke::NONE,
                ));
            }
        }
    }
}

/// The dark checker cells, each cut to the circle, as convex polygons.
fn dark_cells(center: Pos2, radius: f32) -> Vec<Vec<Pos2>> {
    let outline = circle(center, radius);
    let side = radius * 2.0 / f32::from(CHECKER_SPLIT);
    let origin = center - vec2(radius, radius);
    let mut cells = Vec::new();
    for row in 0..CHECKER_SPLIT {
        for col in 0..CHECKER_SPLIT {
            // The top-left cell is dark, as the page's conic pattern draws it.
            if (row + col) % 2 == 1 {
                continue;
            }
            let min = origin + vec2(f32::from(col), f32::from(row)) * side;
            let cut = clip_to_rect(&outline, Rect::from_min_size(min, vec2(side, side)));
            if cut.len() >= 3 {
                cells.push(cut);
            }
        }
    }
    cells
}

/// The circle outline as a convex polygon.
#[expect(
    clippy::cast_precision_loss,
    reason = "the outline has a few dozen points"
)]
fn circle(center: Pos2, radius: f32) -> Vec<Pos2> {
    (0..OUTLINE_POINTS)
        .map(|i| {
            let angle = std::f32::consts::TAU * i as f32 / OUTLINE_POINTS as f32;
            center + radius * vec2(angle.cos(), angle.sin())
        })
        .collect()
}

/// Cut a convex polygon to `rect` (Sutherland-Hodgman, one edge at a time).
fn clip_to_rect(polygon: &[Pos2], rect: Rect) -> Vec<Pos2> {
    // Each edge gives how far a point lies inside it; below zero is outside.
    let edges: [fn(Pos2, Rect) -> f32; 4] = [
        |p, r| p.x - r.min.x,
        |p, r| r.max.x - p.x,
        |p, r| p.y - r.min.y,
        |p, r| r.max.y - p.y,
    ];
    let mut points = polygon.to_vec();
    for inside in edges {
        let Some(&last) = points.last() else {
            break;
        };
        let mut out = Vec::with_capacity(points.len() + 1);
        let mut prev = last;
        for &point in &points {
            let (a, b) = (inside(prev, rect), inside(point, rect));
            if (a >= 0.0) != (b >= 0.0) {
                let t = a / (a - b);
                out.push(pos2(
                    prev.x + (point.x - prev.x) * t,
                    prev.y + (point.y - prev.y) * t,
                ));
            }
            if b >= 0.0 {
                out.push(point);
            }
            prev = point;
        }
        points = out;
    }
    points
}

/// The ring around a round filter swatch: the coral focus ring while it has
/// keyboard focus, else the light ring when the bar holds it, else the raised
/// step on hover, and none at rest.
#[must_use]
pub const fn ring(held: bool, focused: bool, hovered: bool) -> Option<Stroke> {
    let color = if focused {
        tokens::CORAL_STRONG
    } else if held {
        tokens::TEXT_PRIMARY
    } else if hovered {
        tokens::SURFACE_HIGHLIGHT
    } else {
        return None;
    };
    Some(Stroke {
        width: RING_WIDTH,
        color,
    })
}

/// The ring width, in points.
pub const RING_WIDTH: f32 = tokens::FOCUS_RING_WIDTH;
/// The gap between the swatch edge and the ring, in points.
pub const RING_GAP: f32 = 2.0;

#[cfg(test)]
mod tests;
