//! Tests for the mosaic layout and paint.

use std::path::PathBuf;

use super::{MosaicCell, columns, drawn, grid_cells, more_hint, places, show};
use crate::preview::image::PreviewImage;
use egui::{Rect, pos2, vec2};

/// A 216 by 316 point area at (10, 20). Gaps of 8 split it into whole
/// numbers for one, two, and three columns and rows.
fn area() -> Rect {
    Rect::from_min_size(pos2(10.0, 20.0), vec2(216.0, 316.0))
}

/// A cell at `x`, `y` relative to the area, `w` wide and `h` tall.
fn cell(x: f32, y: f32, w: f32, h: f32) -> Rect {
    Rect::from_min_size(pos2(10.0 + x, 20.0 + y), vec2(w, h))
}

/// Whether two rect lists match within a hundredth of a point.
fn close(got: &[Rect], want: &[Rect]) -> bool {
    got.len() == want.len()
        && got
            .iter()
            .zip(want)
            .all(|(g, w)| (g.min - w.min).length() < 0.01 && (g.max - w.max).length() < 0.01)
}

#[test]
fn the_columns_follow_the_readme_rule_then_the_square_root() {
    let got: Vec<usize> = (2..=17).map(columns).collect();
    assert_eq!(got, [1, 2, 2, 2, 2, 3, 3, 3, 4, 4, 4, 4, 4, 4, 4, 5]);
    assert_eq!(columns(100), 10);
}

#[test]
fn two_images_stack_in_one_column() {
    let want = [cell(0.0, 0.0, 216.0, 154.0), cell(0.0, 162.0, 216.0, 154.0)];
    assert!(close(&grid_cells(2, 9, area()), &want));
}

#[test]
fn three_images_start_with_a_cell_two_columns_wide() {
    let want = [
        cell(0.0, 0.0, 216.0, 154.0),
        cell(0.0, 162.0, 104.0, 154.0),
        cell(112.0, 162.0, 104.0, 154.0),
    ];
    assert!(close(&grid_cells(3, 9, area()), &want));
}

#[test]
fn seven_images_give_the_two_top_rows_a_wide_cell() {
    let got = places(7);
    assert_eq!(
        got,
        [
            (0, 0, 2),
            (0, 2, 1),
            (1, 0, 2),
            (1, 2, 1),
            (2, 0, 1),
            (2, 1, 1),
            (2, 2, 1)
        ]
    );
}

#[test]
fn every_count_fills_each_row_with_no_empty_slot() {
    for count in 1..=100 {
        let cols = columns(count);
        let mut per_row = std::collections::BTreeMap::new();
        for (row, _, span) in places(count) {
            *per_row.entry(row).or_insert(0) += span;
        }
        assert_eq!(places(count).len(), count, "{count} cells");
        for (row, used) in per_row {
            assert_eq!(
                used, cols,
                "{count} images: row {row} uses {used} of {cols}"
            );
        }
    }
}

#[test]
fn the_cells_fill_the_area_for_two_three_six_and_nine() {
    for count in [2, 3, 6, 9] {
        let cells = grid_cells(count, 9, area());
        let bounds = cells.iter().copied().reduce(Rect::union).expect("cells");
        assert!(
            (bounds.min - area().min).length() < 0.01 && (bounds.max - area().max).length() < 0.01,
            "{count}: {bounds:?}"
        );
        // Each row runs from the left edge to the right edge, one gap
        // between neighbours, so no slot is left empty.
        let mut tops: Vec<f32> = cells.iter().map(|c| c.min.y).collect();
        tops.dedup_by(|a, b| (*a - *b).abs() < 0.01);
        for top in tops {
            let mut row: Vec<Rect> = cells
                .iter()
                .copied()
                .filter(|c| (c.min.y - top).abs() < 0.01)
                .collect();
            row.sort_by(|a, b| a.min.x.total_cmp(&b.min.x));
            assert!(
                (row[0].min.x - area().min.x).abs() < 0.01,
                "{count}: {row:?}"
            );
            assert!(
                (row[row.len() - 1].max.x - area().max.x).abs() < 0.01,
                "{count}: {row:?}"
            );
            for pair in row.windows(2) {
                assert!(
                    (pair[1].min.x - pair[0].max.x - 8.0).abs() < 0.01,
                    "{count}: {row:?}"
                );
            }
        }
    }
}

#[test]
fn twelve_images_are_cut_to_the_cap_of_nine() {
    assert_eq!(grid_cells(12, 9, area()).len(), 9);
    assert_eq!(grid_cells(12, 4, area()).len(), 4);
}

#[test]
fn no_images_or_too_little_room_give_no_cells() {
    assert_eq!(grid_cells(0, 9, area()), Vec::<Rect>::new());
    let thin = Rect::from_min_size(pos2(0.0, 0.0), vec2(16.0, 300.0));
    assert_eq!(grid_cells(9, 9, thin), Vec::<Rect>::new());
}

#[test]
fn the_hint_counts_the_selected_images_the_mosaic_does_not_draw() {
    assert_eq!(more_hint(9, 9), None);
    assert_eq!(more_hint(20, 9).as_deref(), Some("+11 more"));
    assert_eq!(more_hint(5, 3).as_deref(), Some("+2 more"));
}

fn cells(count: usize) -> Vec<MosaicCell> {
    (0..count)
        .map(|i| MosaicCell {
            image: PreviewImage {
                texture: egui::TextureId::User(3),
                size: (100, 50),
            },
            path: PathBuf::from(format!("/a/{i}.psd")),
        })
        .collect()
}

#[test]
fn drawn_is_the_cell_count_held_to_the_cap() {
    assert_eq!(drawn(&cells(3), 9), 3);
    assert_eq!(drawn(&cells(12), 9), 9);
}

#[test]
fn each_cell_fits_its_picture_whole_on_the_field_fill_with_its_badge() {
    let ctx = egui::Context::default();
    crate::fonts::install(&ctx);
    let input = egui::RawInput {
        screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(400.0, 600.0))),
        ..Default::default()
    };
    let output = ctx.run_ui(input, |ui| {
        show(ui, &cells(2), 9, 400.0);
    });
    let (mut fills, mut pictures, mut badges) = (Vec::new(), Vec::new(), Vec::new());
    for clipped in &output.shapes {
        match &clipped.shape {
            egui::Shape::Rect(r) if r.brush.is_some() => pictures.push(r.rect),
            egui::Shape::Rect(r) if r.fill == crate::tokens::BG_TERTIARY => fills.push(r.rect),
            egui::Shape::Text(t) => badges.push(t.galley.job.text.clone()),
            _ => {}
        }
    }
    output.drop_without_applying_deltas();
    assert_eq!(fills.len(), 2);
    let want: Vec<Rect> = fills
        .iter()
        .map(|cell| crate::preview::image::fit_rect((100, 50), *cell))
        .collect();
    assert_eq!(pictures, want);
    for (picture, cell) in pictures.iter().zip(&fills) {
        assert!(
            cell.contains_rect(*picture),
            "{picture:?} is cut by {cell:?}"
        );
    }
    assert_eq!(badges, ["PSD", "PSD"]);
}
