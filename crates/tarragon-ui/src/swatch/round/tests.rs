//! Tests for the round swatch painter.

use super::{clip_to_rect, dark_cells};
use egui::{Pos2, Rect, pos2, vec2};

/// The area of a polygon (shoelace formula).
fn area(points: &[Pos2]) -> f32 {
    let n = points.len();
    let twice: f32 = (0..n)
        .map(|i| {
            let (a, b) = (points[i], points[(i + 1) % n]);
            a.x * b.y - b.x * a.y
        })
        .sum();
    twice.abs() / 2.0
}

#[test]
fn the_dark_cells_cover_half_the_circle_and_stay_inside_it() {
    let (center, radius) = (pos2(50.0, 50.0), 12.0);
    let cells = dark_cells(center, radius);
    assert_eq!(cells.len(), 8, "half of the 4 by 4 cells are dark");
    for cell in &cells {
        for point in cell {
            assert!(point.distance(center) <= radius + 1e-3, "{point:?}");
        }
    }
    let dark: f32 = cells.iter().map(|c| area(c)).sum();
    let half = std::f32::consts::PI * radius * radius / 2.0;
    assert!((dark - half).abs() / half < 0.02, "{dark} against {half}");
}

#[test]
fn the_top_left_cell_is_dark_as_the_page_draws_it() {
    let (center, radius) = (pos2(50.0, 50.0), 12.0);
    let top_left = Rect::from_min_size(center - vec2(radius, radius), vec2(6.0, 6.0));
    let inner = Rect::from_min_size(center - vec2(6.0, 6.0), vec2(6.0, 6.0));
    let next = Rect::from_min_size(center - vec2(6.0, radius), vec2(6.0, 6.0));
    let holds = |cell: &Vec<Pos2>, rect: Rect| cell.iter().all(|p| rect.expand(1e-3).contains(*p));
    let cells = dark_cells(center, radius);
    assert!(cells.iter().any(|c| holds(c, top_left)));
    assert!(cells.iter().any(|c| holds(c, inner)));
    assert!(
        !cells.iter().any(|c| holds(c, next)),
        "its right neighbour is light"
    );
}

#[test]
fn a_polygon_inside_the_rect_is_kept_and_one_outside_is_dropped() {
    let square = [
        pos2(1.0, 1.0),
        pos2(3.0, 1.0),
        pos2(3.0, 3.0),
        pos2(1.0, 3.0),
    ];
    let rect = Rect::from_min_max(pos2(0.0, 0.0), pos2(4.0, 4.0));
    assert_eq!(clip_to_rect(&square, rect), square);
    let far = Rect::from_min_max(pos2(10.0, 10.0), pos2(12.0, 12.0));
    assert_eq!(clip_to_rect(&square, far), [] as [Pos2; 0]);
    let half = Rect::from_min_max(pos2(2.0, 0.0), pos2(4.0, 4.0));
    assert!((area(&clip_to_rect(&square, half)) - 2.0).abs() < 1e-4);
}
