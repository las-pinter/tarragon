//! Tests for the colour square painter.

use super::{CHECKER_CELL, SIZE, Swatch, bucket_color, checker_cells, show};
use crate::tokens;
use egui::{Color32, Context, CornerRadius, Pos2, Rect, vec2};
use std::collections::HashSet;
use tarragon_core::color::ColorBucket;

fn input() -> egui::RawInput {
    egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(200.0, 200.0))),
        ..Default::default()
    }
}

fn square() -> Rect {
    Rect::from_min_size(Pos2::new(10.0, 20.0), vec2(SIZE, SIZE))
}

#[test]
fn a_24_point_square_has_sixteen_cells_in_row_order() {
    let cells = checker_cells(square(), CHECKER_CELL, 4);
    assert_eq!(cells.len(), 16);
    for (i, cell) in cells.iter().enumerate() {
        let (col, row) = (u8::try_from(i % 4).unwrap(), u8::try_from(i / 4).unwrap());
        let want = Rect::from_min_size(
            Pos2::new(10.0 + 6.0 * f32::from(col), 20.0 + 6.0 * f32::from(row)),
            vec2(6.0, 6.0),
        );
        assert_eq!(cell.rect, want, "cell {i}");
    }
}

#[test]
fn neighbouring_cells_alternate_and_the_first_is_light() {
    let cells = checker_cells(square(), CHECKER_CELL, 4);
    assert!(!cells[0].dark);
    for (i, cell) in cells.iter().enumerate() {
        let (col, row) = (i % 4, i / 4);
        assert_eq!(cell.dark, (col + row) % 2 == 1, "cell {i}");
    }
}

#[test]
fn only_the_four_square_corners_are_rounded() {
    let cells = checker_cells(square(), CHECKER_CELL, 4);
    for (i, cell) in cells.iter().enumerate() {
        let want = CornerRadius {
            nw: if i == 0 { 4 } else { 0 },
            ne: if i == 3 { 4 } else { 0 },
            sw: if i == 12 { 4 } else { 0 },
            se: if i == 15 { 4 } else { 0 },
        };
        assert_eq!(cell.corners, want, "cell {i}");
    }
}

#[test]
fn edge_cells_are_cut_to_the_square() {
    let rect = Rect::from_min_size(Pos2::ZERO, vec2(10.0, 7.0));
    let cells = checker_cells(rect, 6.0, 0);
    assert_eq!(cells.len(), 4);
    assert_eq!(cells[1].rect.width(), 4.0);
    assert_eq!(cells[2].rect.height(), 1.0);
    assert!(cells.iter().all(|c| rect.contains_rect(c.rect)));
}

#[test]
fn an_empty_square_or_cell_gives_no_cells() {
    assert_eq!(
        checker_cells(Rect::from_min_size(Pos2::ZERO, vec2(0.0, 5.0)), 6.0, 4),
        vec![]
    );
    assert_eq!(checker_cells(square(), 0.0, 4), vec![]);
}

#[test]
fn the_twelve_squares_are_the_buckets_then_transparent() {
    assert_eq!(Swatch::ALL.len(), 12);
    for (swatch, bucket) in Swatch::ALL.iter().zip(ColorBucket::ALL) {
        assert_eq!(*swatch, Swatch::Bucket(bucket));
    }
    assert_eq!(Swatch::ALL[11], Swatch::Transparent);
    assert_eq!(bucket_color(ColorBucket::Pink), tokens::BUCKET_PINK);
}

/// The rect and fill of every plain rectangle in a frame that shows `swatch`.
fn painted(swatch: Swatch, opacity: f32) -> Vec<(Rect, Color32)> {
    use egui::epaint::Shape;
    fn collect(shape: &Shape, found: &mut Vec<(Rect, Color32)>) {
        match shape {
            Shape::Vec(shapes) => shapes.iter().for_each(|s| collect(s, found)),
            Shape::Rect(r) => found.push((r.rect, r.fill)),
            _ => {}
        }
    }
    let ctx = Context::default();
    let output = ctx.run_ui(input(), |ui| {
        show(ui, swatch, SIZE, opacity);
    });
    let mut found = Vec::new();
    for clipped in &output.shapes {
        collect(&clipped.shape, &mut found);
    }
    output.drop_without_applying_deltas();
    found
}

#[test]
fn a_bucket_square_paints_its_colour_with_the_opacity_applied() {
    for opacity in [1.0, 0.5, 0.3] {
        let found = painted(Swatch::Bucket(ColorBucket::Blue), opacity);
        let want = tokens::BUCKET_BLUE.gamma_multiply(opacity);
        let hits: Vec<_> = found.iter().filter(|(_, fill)| *fill == want).collect();
        assert_eq!(hits.len(), 1, "opacity {opacity}: {found:?}");
        assert_eq!(hits[0].0.size(), vec2(SIZE, SIZE));
    }
}

#[test]
fn a_transparent_square_paints_each_cell_in_its_grey_with_opacity() {
    let found = painted(Swatch::Transparent, 0.5);
    let light = tokens::CHECKER_LIGHT.gamma_multiply(0.5);
    let dark = tokens::CHECKER_DARK.gamma_multiply(0.5);
    assert_eq!(found.len(), 16);
    for (i, (rect, fill)) in found.iter().enumerate() {
        let want = if (i / 4 + i % 4) % 2 == 1 {
            dark
        } else {
            light
        };
        assert_eq!(*fill, want, "cell {i} at {rect:?}");
    }
}

#[test]
fn a_cell_size_that_is_not_exact_in_binary_leaves_no_sliver() {
    let rect = Rect::from_min_size(Pos2::new(0.3, 0.7), vec2(1.0, 1.0));
    let cells = checker_cells(rect, 0.1, 0);
    assert_eq!(cells.len(), 100);
    assert!(
        cells
            .iter()
            .all(|c| c.rect.width() > 0.09 && c.rect.height() > 0.09)
    );
    assert_eq!(cells[99].rect.max, rect.max);
    assert_eq!(cells[0].rect.min, rect.min);
    // Neighbours share their edge exactly, so no gap or overlap appears.
    assert_eq!(cells[0].rect.max.x, cells[1].rect.min.x);
    assert_eq!(cells[0].rect.max.y, cells[10].rect.min.y);
}

#[test]
fn a_cut_grid_keeps_the_pattern_and_rounds_its_four_corners() {
    let rect = Rect::from_min_size(Pos2::ZERO, vec2(10.0, 7.0));
    let cells = checker_cells(rect, 6.0, 4);
    let dark: Vec<bool> = cells.iter().map(|c| c.dark).collect();
    assert_eq!(dark, [false, true, true, false]);
    let corners: Vec<CornerRadius> = cells.iter().map(|c| c.corners).collect();
    let one = |nw, ne, sw, se| CornerRadius { nw, ne, sw, se };
    assert_eq!(
        corners,
        [
            one(4, 0, 0, 0),
            one(0, 4, 0, 0),
            one(0, 0, 4, 0),
            one(0, 0, 0, 4)
        ]
    );
}

#[test]
fn the_eleven_buckets_have_eleven_distinct_colours() {
    let colors: HashSet<Color32> = ColorBucket::ALL.into_iter().map(bucket_color).collect();
    assert_eq!(colors.len(), 11);
}

#[test]
fn show_takes_the_given_size_and_senses_clicks() {
    let ctx = Context::default();
    let mut seen = None;
    let output = ctx.run_ui(input(), |ui| {
        let response = show(ui, Swatch::Transparent, 30.0, 1.0);
        seen = Some((response.rect.size(), response.sense));
    });
    output.drop_without_applying_deltas();
    let (size, sense) = seen.expect("the closure ran");
    assert_eq!(size, vec2(30.0, 30.0));
    assert!(sense.senses_click());
}
