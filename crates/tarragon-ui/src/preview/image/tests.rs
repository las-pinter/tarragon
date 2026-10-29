//! Tests for the picture fit.

use super::*;
use egui::epaint::RectShape;
use egui::{CornerRadius, TextureId, pos2};

fn area() -> Rect {
    Rect::from_min_size(pos2(10.0, 20.0), vec2(400.0, 200.0))
}

#[test]
fn a_wide_picture_fills_the_width() {
    let got = fit_rect((800, 200), area());
    assert_eq!(
        got,
        Rect::from_min_max(pos2(10.0, 70.0), pos2(410.0, 170.0))
    );
}

#[test]
fn a_tall_picture_fills_the_height() {
    let got = fit_rect((100, 400), area());
    assert_eq!(
        got,
        Rect::from_min_max(pos2(185.0, 20.0), pos2(235.0, 220.0))
    );
}

#[test]
fn a_small_wide_picture_grows_until_its_width_fills_the_area() {
    let got = fit_rect((100, 50), area());
    assert_eq!(
        got,
        Rect::from_min_max(pos2(10.0, 20.0), pos2(410.0, 220.0))
    );
}

#[test]
fn a_small_tall_picture_grows_until_its_height_fills_the_area() {
    let got = fit_rect((20, 100), area());
    assert_eq!(
        got,
        Rect::from_min_max(pos2(190.0, 20.0), pos2(230.0, 220.0))
    );
}

#[test]
fn a_picture_of_the_area_size_fills_it() {
    assert_eq!(fit_rect((400, 200), area()), area());
}

#[test]
fn a_zero_size_gives_an_empty_rect_at_the_centre() {
    let centre = area().center();
    let empty = Rect::from_center_size(centre, Vec2::ZERO);
    assert_eq!(fit_rect((0, 50), area()), empty);
    assert_eq!(fit_rect((50, 0), area()), empty);
    let flat = Rect::from_min_size(pos2(0.0, 0.0), vec2(0.0, 30.0));
    assert_eq!(fit_rect((50, 50), flat).size(), Vec2::ZERO);
}

#[test]
fn the_box_fills_what_the_content_below_leaves_and_never_drops_under_the_minimum() {
    let min = tokens::PREVIEW_IMAGE_HEIGHT;
    assert_eq!(box_height(900.0, 200.0), 699.5);
    assert_eq!(box_height(400.0, 200.0), min);
    assert_eq!(box_height(900.0, 0.0), 899.5);
}

/// Run one frame of `show_box` for a 64 x 48 picture and give the box rect and
/// the painted shapes.
fn paint_box(height: f32) -> (Rect, Vec<egui::Shape>) {
    let ctx = egui::Context::default();
    let input = egui::RawInput {
        screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(400.0, 600.0))),
        ..Default::default()
    };
    let image = PreviewImage {
        texture: TextureId::User(7),
        size: (64, 48),
    };
    let mut rect = Rect::NOTHING;
    let output = ctx.run_ui(input, |ui| rect = show_box(ui, &image, height));
    let shapes = output.shapes.iter().map(|c| c.shape.clone()).collect();
    output.drop_without_applying_deltas();
    (rect, shapes)
}

/// The textured rects among `shapes`.
fn textured(shapes: &[egui::Shape]) -> Vec<RectShape> {
    shapes
        .iter()
        .flat_map(|s| match s {
            egui::Shape::Vec(inner) => textured(inner),
            egui::Shape::Rect(r) if r.brush.is_some() => vec![r.clone()],
            _ => Vec::new(),
        })
        .collect()
}

#[test]
fn the_picture_is_inset_from_the_box_and_keeps_its_rounded_corners() {
    let (rect, shapes) = paint_box(300.0);
    let pictures = textured(&shapes);
    assert_eq!(pictures.len(), 1);
    let inner = rect.shrink(tokens::PREVIEW_IMAGE_INSET);
    assert_eq!(pictures[0].rect, fit_rect((64, 48), inner));
    assert_eq!(
        pictures[0].corner_radius,
        CornerRadius::same(tokens::RADIUS_L)
    );
}

#[test]
fn a_picture_that_fills_the_inner_area_stays_16_points_off_every_box_edge() {
    // 64 x 48 fits exactly in a 4:3 inner area: 400 - 32 = 368 wide, 276 tall.
    let (rect, shapes) = paint_box(276.0 + 32.0);
    let picture = textured(&shapes)[0].rect;
    assert_eq!(picture.min - rect.min, vec2(16.0, 16.0));
    assert_eq!(rect.max - picture.max, vec2(16.0, 16.0));
}
