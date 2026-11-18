//! Tests for the sidebar row marks.

use super::*;
use egui::{Context, RawInput};

#[test]
fn the_icon_body_and_tab_have_the_stylesheet_sizes() {
    let (body, tab) = icon_rects(pos2(10.0, 20.0));
    assert_eq!(body.size(), vec2(13.0, 10.0));
    assert_eq!(tab.size(), vec2(6.0, 3.0));
    assert_eq!(tab.left_bottom(), body.left_top());
}

#[test]
fn a_closed_caret_points_right() {
    let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(10.0, 10.0));
    let [a, tip, b] = caret_points(rect, false);
    assert_eq!(a.x, b.x);
    assert!((b.y - a.y - 6.0).abs() < f32::EPSILON);
    assert!((tip.x - a.x - 4.0).abs() < f32::EPSILON);
    assert!((tip.y - rect.center().y).abs() < f32::EPSILON);
}

#[test]
fn an_open_caret_points_down() {
    let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(10.0, 10.0));
    let [a, b, tip] = caret_points(rect, true);
    assert_eq!(a.y, b.y);
    assert!((b.x - a.x - 6.0).abs() < f32::EPSILON);
    assert!((tip.y - a.y - 4.0).abs() < f32::EPSILON);
    assert!((tip.x - rect.center().x).abs() < f32::EPSILON);
}

#[test]
fn both_painters_run_in_a_headless_frame() {
    let ctx = Context::default();
    let raw = RawInput {
        screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(200.0, 100.0))),
        ..Default::default()
    };
    let output = ctx.run_ui(raw, |ui| {
        let painter = ui.painter();
        let slot = Rect::from_min_size(pos2(4.0, 20.0), vec2(10.0, 10.0));
        paint_caret(painter, slot, true, Color32::WHITE);
        paint_caret(
            painter,
            slot.translate(vec2(20.0, 0.0)),
            false,
            Color32::WHITE,
        );
        paint_folder_icon(painter, pos2(50.0, 20.0), Color32::WHITE);
    });
    let mut rects = Vec::new();
    let mut paths = 0;
    for clipped in &output.shapes {
        match &clipped.shape {
            Shape::Rect(rect) => rects.push((rect.fill, rect.corner_radius)),
            Shape::Path(_) => paths += 1,
            _ => {}
        }
    }
    let tab = CornerRadius {
        nw: 1,
        ne: 1,
        sw: 0,
        se: 0,
    };
    assert_eq!(paths, 2, "one path per caret");
    assert_eq!(
        rects,
        [
            (Color32::WHITE, CornerRadius::same(2)),
            (Color32::WHITE, tab)
        ]
    );
    output.drop_without_applying_deltas();
}
