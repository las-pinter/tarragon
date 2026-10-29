//! Tests for the picture fit.

use super::*;
use egui::pos2;

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
