//! Tests for the mosaic layout.

use super::{columns, grid_cells, grid_height, more_hint};
use egui::{Rect, pos2};

/// A 116 point wide area at (10, 20). The 8 point padding leaves 100 points,
/// and gaps of 8 divide that into whole numbers.
fn area() -> Rect {
    Rect::from_min_size(pos2(10.0, 20.0), egui::vec2(116.0, 516.0))
}

/// A 120 point tall cell at `x`, `y` (relative to the padded grid) and `w`
/// wide. The grid starts 8 points inside the area.
fn cell(x: f32, y: f32, w: f32) -> Rect {
    Rect::from_min_size(pos2(18.0 + x, 28.0 + y), egui::vec2(w, 120.0))
}

#[test]
fn the_columns_are_the_ceiling_of_the_square_root() {
    let got: Vec<usize> = (0..=10).map(columns).collect();
    assert_eq!(got, [1, 1, 2, 2, 2, 3, 3, 3, 3, 3, 4]);
}

#[test]
fn two_images_sit_side_by_side() {
    assert_eq!(
        grid_cells(2, 9, area()),
        [cell(0.0, 0.0, 46.0), cell(54.0, 0.0, 46.0)]
    );
}

#[test]
fn three_images_fill_two_columns_and_leave_a_hole() {
    assert_eq!(
        grid_cells(3, 9, area()),
        [
            cell(0.0, 0.0, 46.0),
            cell(54.0, 0.0, 46.0),
            cell(0.0, 128.0, 46.0)
        ]
    );
}

#[test]
fn four_images_make_a_square() {
    let cells = grid_cells(4, 9, area());
    assert_eq!(cells.len(), 4);
    assert_eq!(cells[3], cell(54.0, 128.0, 46.0));
}

#[test]
fn five_images_use_three_columns() {
    let cells = grid_cells(5, 9, area());
    assert_eq!(cells.len(), 5);
    assert_eq!(cells[0], cell(0.0, 0.0, 28.0));
    assert_eq!(cells[2], cell(72.0, 0.0, 28.0));
    assert_eq!(cells[4], cell(36.0, 128.0, 28.0));
}

#[test]
fn nine_images_make_a_three_by_three_grid() {
    let cells = grid_cells(9, 9, area());
    assert_eq!(cells.len(), 9);
    assert_eq!(cells[8], cell(72.0, 256.0, 28.0));
}

#[test]
fn twelve_images_are_cut_to_the_cap_of_nine() {
    assert_eq!(grid_cells(12, 9, area()), grid_cells(9, 9, area()));
}

#[test]
fn a_cap_of_four_gives_two_columns() {
    assert_eq!(grid_cells(12, 4, area()), grid_cells(4, 9, area()));
}

#[test]
fn no_images_or_no_area_give_no_cells() {
    let none: Vec<Rect> = Vec::new();
    let point = Rect::from_min_size(pos2(5.0, 5.0), egui::Vec2::ZERO);
    let flat = Rect::from_min_size(pos2(0.0, 0.0), egui::vec2(100.0, 0.0));
    let thin = Rect::from_min_size(pos2(0.0, 0.0), egui::vec2(16.0, 500.0));
    assert_eq!(grid_cells(0, 9, area()), none);
    assert_eq!(grid_cells(4, 0, area()), none);
    assert_eq!(grid_cells(4, 9, point), none);
    assert_eq!(grid_cells(4, 9, flat), none);
    assert_eq!(grid_cells(4, 9, thin), none);
}

#[test]
fn the_grid_height_counts_rows_gaps_and_padding() {
    assert_eq!(grid_height(0, 9), 0.0);
    assert_eq!(grid_height(2, 9), 136.0);
    assert_eq!(grid_height(3, 9), 264.0);
    assert_eq!(grid_height(9, 9), 3.0 * 120.0 + 2.0 * 8.0 + 16.0);
    assert_eq!(grid_height(12, 4), 264.0);
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
