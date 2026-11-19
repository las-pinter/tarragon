//! Tests for the mosaic layout.

use super::{aspect, columns, grid_cells, grid_height, more_hint, row_sizes};
use egui::{Rect, pos2, vec2};

/// A 116 point wide area at (10, 20). The 8 point padding leaves 100 points,
/// and gaps of 8 divide that into whole numbers.
fn area() -> Rect {
    Rect::from_min_size(pos2(10.0, 20.0), vec2(116.0, 0.0))
}

/// `count` square pictures.
fn squares(count: usize) -> Vec<(u32, u32)> {
    vec![(50, 50); count]
}

/// A cell at `x`, `y` (relative to the padded grid), `w` wide and `h` tall.
/// The grid starts 8 points inside the area.
fn cell(x: f32, y: f32, w: f32, h: f32) -> Rect {
    Rect::from_min_size(pos2(18.0 + x, 28.0 + y), vec2(w, h))
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
fn the_columns_are_the_ceiling_of_the_square_root() {
    let got: Vec<usize> = (0..=10).map(columns).collect();
    assert_eq!(got, [1, 1, 2, 2, 2, 3, 3, 3, 3, 3, 4]);
}

#[test]
fn the_images_spread_evenly_over_the_rows_fuller_rows_first() {
    let got: Vec<Vec<usize>> = (0..=9).map(row_sizes).collect();
    let want: [&[usize]; 10] = [
        &[],
        &[1],
        &[2],
        &[2, 1],
        &[2, 2],
        &[3, 2],
        &[3, 3],
        &[3, 2, 2],
        &[3, 3, 2],
        &[3, 3, 3],
    ];
    assert_eq!(got, want);
}

#[test]
fn two_squares_share_the_full_width() {
    let want = [cell(0.0, 0.0, 46.0, 46.0), cell(54.0, 0.0, 46.0, 46.0)];
    assert!(close(&grid_cells(&squares(2), 9, area()), &want));
}

#[test]
fn five_images_give_a_row_of_three_and_a_full_width_row_of_two() {
    let cells = grid_cells(&squares(5), 9, area());
    let want = [
        cell(0.0, 0.0, 28.0, 28.0),
        cell(36.0, 0.0, 28.0, 28.0),
        cell(72.0, 0.0, 28.0, 28.0),
        cell(0.0, 36.0, 46.0, 46.0),
        cell(54.0, 36.0, 46.0, 46.0),
    ];
    assert!(close(&cells, &want), "{cells:?}");
}

#[test]
fn each_cell_takes_the_shape_of_its_picture() {
    let cells = grid_cells(&[(200, 100), (100, 100)], 9, area());
    let height = 92.0 / 3.0;
    let want = [
        cell(0.0, 0.0, 2.0 * height, height),
        cell(2.0 * height + 8.0, 0.0, height, height),
    ];
    assert!(close(&cells, &want), "{cells:?}");
}

#[test]
fn a_row_taller_than_the_picture_box_keeps_its_height_and_is_centred() {
    let wide = Rect::from_min_size(pos2(10.0, 20.0), vec2(516.0, 0.0));
    let cells = grid_cells(&squares(3), 9, wide);
    let last = cells[2];
    assert!((last.height() - crate::tokens::PREVIEW_IMAGE_HEIGHT).abs() < 0.01);
    assert!((last.center().x - wide.center().x).abs() < 0.01, "{last:?}");
}

#[test]
fn an_extreme_or_empty_picture_gets_a_bounded_cell_shape() {
    assert!((aspect((1000, 10)) - 4.0).abs() < f32::EPSILON);
    assert!((aspect((10, 1000)) - 0.25).abs() < f32::EPSILON);
    assert!((aspect((0, 40)) - 1.0).abs() < f32::EPSILON);
    assert!((aspect((64, 48)) - 4.0 / 3.0).abs() < 1e-6);
}

#[test]
fn twelve_images_are_cut_to_the_cap_of_nine() {
    assert_eq!(
        grid_cells(&squares(12), 9, area()),
        grid_cells(&squares(9), 9, area())
    );
}

#[test]
fn no_images_no_cap_or_no_width_give_no_cells() {
    let none: Vec<Rect> = Vec::new();
    let thin = Rect::from_min_size(pos2(0.0, 0.0), vec2(16.0, 500.0));
    assert_eq!(grid_cells(&[], 9, area()), none);
    assert_eq!(grid_cells(&squares(4), 0, area()), none);
    assert_eq!(grid_cells(&squares(4), 9, thin), none);
}

#[test]
fn the_grid_height_reaches_the_last_row_and_the_padding() {
    assert_eq!(grid_height(&[], area()), 0.0);
    let cells = grid_cells(&squares(5), 9, area());
    assert!((grid_height(&cells, area()) - (8.0 + 28.0 + 8.0 + 46.0 + 8.0)).abs() < 0.01);
}

#[test]
fn the_hint_counts_the_selected_images_the_mosaic_does_not_draw() {
    assert_eq!(more_hint(9, 9), None);
    assert_eq!(more_hint(20, 9).as_deref(), Some("+11 more"));
    assert_eq!(more_hint(5, 3).as_deref(), Some("+2 more"));
    assert_eq!(more_hint(3, 3), None);
}

#[test]
fn each_painted_picture_carries_the_cell_corner_radius() {
    use super::{PreviewImage, show};
    let ctx = egui::Context::default();
    let input = egui::RawInput {
        screen_rect: Some(Rect::from_min_size(
            pos2(0.0, 0.0),
            egui::vec2(400.0, 600.0),
        )),
        ..Default::default()
    };
    let images = [PreviewImage {
        texture: egui::TextureId::User(3),
        size: (50, 50),
    }; 2];
    let output = ctx.run_ui(input, |ui| show(ui, &images, 2, 9));
    let (mut cells, mut pictures) = (Vec::new(), Vec::new());
    for clipped in &output.shapes {
        if let egui::Shape::Rect(r) = &clipped.shape {
            if r.brush.is_some() {
                pictures.push((r.rect, r.corner_radius));
            } else if r.fill == crate::tokens::BG_SECONDARY {
                cells.push(r.rect);
            }
        }
    }
    output.drop_without_applying_deltas();
    let want = egui::CornerRadius::same(crate::tokens::RADIUS_M);
    let expected: Vec<_> = cells
        .iter()
        .map(|cell| (crate::preview::image::fit_rect((50, 50), *cell), want))
        .collect();
    assert_eq!(cells.len(), 2);
    assert_eq!(pictures, expected);
}
