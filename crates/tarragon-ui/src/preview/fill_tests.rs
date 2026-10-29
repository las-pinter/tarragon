//! Tests for the picture box that fills the free height of the pane.

use super::test_support::pill;
use super::{ColorState, Meta, Preview, PreviewImage};
use crate::tokens;
use egui::{Context, CornerRadius, Pos2, Rect, TextureId, ViewportId, vec2};
use std::time::Duration;

/// What one frame showed.
struct Frame {
    /// The picture box, when it was painted.
    picture_box: Option<Rect>,
    /// The height that the Preview had to fill.
    available: f32,
    /// Whether the frame asked for another one at once.
    repaint: bool,
}

fn preview() -> Preview {
    let mut preview = Preview::default();
    preview.set_meta(Some(Meta {
        name: "a.png".into(),
        dimensions: Some((64, 48)),
        size: Some(10),
        format: "PNG".into(),
    }));
    preview.set_image(Some(PreviewImage {
        texture: TextureId::User(1),
        size: (64, 48),
    }));
    preview
}

/// The picture box in a painted shape: the rounded, dark rect.
fn find(shape: &egui::Shape) -> Option<Rect> {
    match shape {
        egui::Shape::Vec(shapes) => shapes.iter().find_map(find),
        egui::Shape::Rect(r)
            if r.corner_radius == CornerRadius::same(tokens::RADIUS_L)
                && r.fill == tokens::BG_SECONDARY =>
        {
            Some(r.rect)
        }
        _ => None,
    }
}

fn frame(ctx: &Context, preview: &mut Preview, height: f32) -> Frame {
    let input = egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(400.0, height))),
        ..Default::default()
    };
    let mut available = 0.0;
    let output = ctx.run_ui(input, |ui| {
        available = ui.available_height();
        preview.show(ui);
    });
    let picture_box = output.shapes.iter().find_map(|c| find(&c.shape));
    let repaint = output.viewport_output[&ViewportId::ROOT].repaint_delay == Duration::ZERO;
    output.drop_without_applying_deltas();
    Frame {
        picture_box,
        available,
        repaint,
    }
}

/// Draw frames until one asks for no repaint, and return it. egui itself asks
/// for a few repaints while a new grid and scroll area find their size.
fn settle(ctx: &Context, preview: &mut Preview, height: f32) -> Frame {
    for _ in 0..8 {
        let last = frame(ctx, preview, height);
        if !last.repaint {
            return last;
        }
    }
    panic!("the layout did not settle");
}

fn context() -> Context {
    let ctx = Context::default();
    crate::theme::apply_theme(&ctx);
    // The first frames only load the fonts and may ask for a repaint.
    frame(&ctx, &mut Preview::default(), 600.0);
    frame(&ctx, &mut Preview::default(), 600.0);
    ctx
}

#[test]
fn a_tall_pane_gives_the_box_all_the_height_the_content_leaves() {
    let ctx = context();
    let mut preview = preview();
    let last = settle(&ctx, &mut preview, 900.0);
    let picture_box = last.picture_box.expect("the box is painted");
    assert!(picture_box.height() > tokens::PREVIEW_IMAGE_HEIGHT);
    assert!(
        (picture_box.height() + preview.below_box + 0.5 - last.available).abs() < 0.01,
        "box {} plus below {} plus the slack is not {}",
        picture_box.height(),
        preview.below_box,
        last.available
    );
}

#[test]
fn a_short_pane_keeps_the_minimum_box_and_overflows_into_the_scroll_area() {
    let ctx = context();
    let mut preview = preview();
    // egui itself keeps asking for frames while a scroll area overflows, so
    // this test draws a fixed number of frames instead of waiting for quiet.
    let mut last = frame(&ctx, &mut preview, 300.0);
    for _ in 0..4 {
        last = frame(&ctx, &mut preview, 300.0);
    }
    let picture_box = last.picture_box.expect("the box is painted");
    assert!((picture_box.height() - tokens::PREVIEW_IMAGE_HEIGHT).abs() < 0.01);
    assert!(
        picture_box.height() + preview.below_box > last.available,
        "the content must be taller than the pane"
    );
}

#[test]
fn a_stable_layout_asks_for_no_more_frames() {
    let ctx = context();
    let mut preview = preview();
    settle(&ctx, &mut preview, 900.0);
    for _ in 0..5 {
        assert!(!frame(&ctx, &mut preview, 900.0).repaint);
    }
}

#[test]
fn more_tags_shrink_the_box_and_the_layout_settles_again() {
    let ctx = context();
    let mut preview = preview();
    let before = settle(&ctx, &mut preview, 900.0);
    let before = before.picture_box.expect("the box is painted").height();

    preview.set_tags(
        (0..40)
            .map(|n| pill(&format!("tag-number-{n}"), ColorState::All))
            .collect(),
    );
    let below = preview.below_box;
    assert!(
        frame(&ctx, &mut preview, 900.0).repaint,
        "a new content height asks for a frame"
    );
    assert!(preview.below_box > below, "the frame measured the tags");
    let after = settle(&ctx, &mut preview, 900.0);
    assert!(
        after.picture_box.expect("the box is painted").height() < before,
        "wrapped tags take room from the box"
    );
}
