//! Tests for the sidebar row marks.

use super::*;
use egui::{Context, RawInput};

/// Whether two points are within a hundredth of a point.
fn near(a: Pos2, b: Pos2) -> bool {
    (a - b).length() < 0.01
}

#[test]
fn the_folder_outline_follows_the_page_grid() {
    let origin = pos2(10.0, 20.0);
    let points = folder_points(origin);
    let at = |x, y| origin + vec2(x, y);
    // The outline starts on the left edge, reaches the tab and its step, and
    // stays inside the 2..14 by 3..13 box of the page path.
    assert!(near(points[0], at(2.0, 4.5)), "{:?}", points[0]);
    assert!(points.iter().any(|p| near(*p, at(6.0, 3.0))));
    assert!(points.iter().any(|p| near(*p, at(7.5, 4.5))));
    assert!(points.iter().any(|p| near(*p, at(14.0, 11.5))));
    assert!(near(*points.last().unwrap(), at(2.0, 11.5)));
    for point in &points {
        let local = *point - origin;
        assert!(
            (1.99..=14.01).contains(&local.x) && (2.99..=13.01).contains(&local.y),
            "{local:?} is outside the folder box"
        );
    }
}

#[test]
fn a_closed_caret_points_right() {
    let [a, tip, b] = caret_points(Pos2::ZERO, false);
    assert!(near(a, pos2(4.5, 3.0)) && near(b, pos2(4.5, 9.0)));
    assert!(near(tip, pos2(7.5, 6.0)));
}

#[test]
fn an_open_caret_points_down() {
    let [a, tip, b] = caret_points(Pos2::ZERO, true);
    assert!((a.y - b.y).abs() < 0.01, "the arms sit level");
    assert!(near(tip, pos2(6.0, 7.5)), "{tip:?}");
    assert!(tip.y > a.y);
}

#[test]
fn every_star_corner_is_seen_from_the_fan_centre() {
    // A fan fills a non-convex shape only when each triangle turns the same
    // way, so no triangle folds back over another.
    let c = vec2(STAR_CENTRE.0, STAR_CENTRE.1);
    for index in 0..STAR.len() {
        let (a, b) = (STAR[index], STAR[(index + 1) % STAR.len()]);
        let (a, b) = (vec2(a.0, a.1) - c, vec2(b.0, b.1) - c);
        assert!(a.x * b.y - a.y * b.x > 0.0, "triangle {index} folds back");
    }
}

#[test]
fn the_painters_run_in_a_headless_frame() {
    let ctx = Context::default();
    let raw = RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(200.0, 100.0))),
        ..Default::default()
    };
    let output = ctx.run_ui(raw, |ui| {
        let painter = ui.painter();
        paint_caret(painter, pos2(4.0, 20.0), true, Color32::WHITE);
        paint_folder_icon(painter, pos2(30.0, 20.0), Color32::WHITE);
        paint_star(painter, pos2(60.0, 20.0), Color32::YELLOW);
    });
    let (mut paths, mut lines, mut meshes) = (0, 0, 0);
    for clipped in &output.shapes {
        match &clipped.shape {
            Shape::Path(path) if path.closed => paths += 1,
            Shape::Path(_) => lines += 1,
            Shape::Mesh(mesh) => {
                meshes += 1;
                assert_eq!(mesh.indices.len(), 30, "ten triangles");
                assert!(mesh.vertices.iter().all(|v| v.color == Color32::YELLOW));
            }
            _ => {}
        }
    }
    assert_eq!((paths, lines, meshes), (1, 1, 1));
    output.drop_without_applying_deltas();
}

#[test]
fn the_icon_origin_centres_the_square_on_whole_points() {
    let rect = Rect::from_min_size(pos2(0.0, 10.3), vec2(100.0, 34.0));
    assert_eq!(icon_origin(rect, 5.0, ICON_SIZE), pos2(5.0, 19.0));
}
