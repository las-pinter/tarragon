//! Tests for the empty hint and the scroll bar of the panel.

use super::{EMPTY_HINT, Meta, Preview, PreviewImage};
use crate::tokens;
use egui::style::ScrollStyle;
use egui::{Context, CornerRadius, Pos2, Rect, Shape, TextureId, vec2};

const WIDTH: f32 = 400.0;

fn context() -> Context {
    let ctx = Context::default();
    crate::theme::apply_theme(&ctx);
    ctx
}

/// Run one frame in a `WIDTH` by `height` window and give every shape.
fn paint(ctx: &Context, preview: &mut Preview, height: f32) -> Vec<Shape> {
    let input = egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(WIDTH, height))),
        ..Default::default()
    };
    let output = ctx.run_ui(input, |ui| preview.show(ui));
    let shapes = output.shapes.iter().map(|c| c.shape.clone()).collect();
    output.drop_without_applying_deltas();
    shapes
}

fn flat(shapes: &[Shape]) -> Vec<&Shape> {
    shapes
        .iter()
        .flat_map(|s| match s {
            Shape::Vec(inner) => flat(inner),
            other => vec![other],
        })
        .collect()
}

#[test]
fn the_empty_hint_is_centred_in_the_pane() {
    let ctx = context();
    let mut preview = Preview::default();
    let mut shapes = Vec::new();
    for _ in 0..3 {
        shapes = paint(&ctx, &mut preview, 600.0);
    }
    let text = flat(&shapes)
        .into_iter()
        .find_map(|s| match s {
            Shape::Text(t) if t.galley.job.text == EMPTY_HINT => Some(t.clone()),
            _ => None,
        })
        .expect("the hint is painted");
    // A centred galley keeps its rect relative to the anchor point.
    let centre = text.galley.rect.translate(text.pos.to_vec2()).center();
    assert!((centre.x - WIDTH / 2.0).abs() <= 0.5, "{centre:?}");
    assert!((centre.y - 300.0).abs() <= 0.5, "{centre:?}");
}

/// The width of the picture box after some frames in a window `height` tall.
fn box_width(height: f32) -> f32 {
    let ctx = context();
    let mut preview = Preview::default();
    preview.set_meta(Some(Meta {
        name: "a.png".into(),
        dimensions: None,
        size: None,
        format: "PNG".into(),
    }));
    preview.set_image(Some(PreviewImage {
        texture: TextureId::User(1),
        size: (64, 48),
    }));
    let mut shapes = Vec::new();
    for _ in 0..4 {
        shapes = paint(&ctx, &mut preview, height);
    }
    flat(&shapes)
        .into_iter()
        .find_map(|s| match s {
            Shape::Rect(r)
                if r.corner_radius == CornerRadius::same(tokens::RADIUS_L)
                    && r.fill == tokens::BG_SECONDARY =>
            {
                Some(r.rect.width())
            }
            _ => None,
        })
        .expect("the box is painted")
}

#[test]
fn a_bar_takes_its_room_only_when_the_content_overflows() {
    let bar = ScrollStyle::solid().bar_width;
    let (tall, short) = (box_width(900.0), box_width(300.0));
    assert_eq!(tall, box_width(2000.0), "a pane with room reserves nothing");
    assert!(tall - short >= bar, "{tall} {short} {bar}");
}

#[test]
fn showing_the_panel_leaves_the_callers_scroll_style_alone() {
    let ctx = context();
    let mut preview = Preview::default();
    preview.set_meta(Some(Meta {
        name: "a.png".into(),
        dimensions: None,
        size: None,
        format: "PNG".into(),
    }));
    let input = egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(WIDTH, 600.0))),
        ..Default::default()
    };
    let mut after = None;
    let output = ctx.run_ui(input, |ui| {
        preview.show(ui);
        after = Some(ui.spacing().scroll);
    });
    output.drop_without_applying_deltas();
    assert_eq!(after, Some(ctx.global_style().spacing.scroll));
}
