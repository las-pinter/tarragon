//! Tests for the error card.

use super::test_support::collect;
use super::{EMPTY_HINT, Meta, Preview, PreviewEvent, PreviewFailure, PreviewImage};
use crate::tokens;
use egui::epaint::Shape;
use egui::{Context, CornerRadius, Event, PointerButton, Pos2, Rect, Stroke, TextureId, vec2};

fn context() -> Context {
    let ctx = Context::default();
    crate::theme::apply_theme(&ctx);
    ctx
}

/// Every painted text with its position.
type Texts = Vec<(String, Pos2)>;

fn failure(message: &str) -> PreviewFailure {
    PreviewFailure {
        name: "poster_v3.psd".into(),
        message: message.into(),
    }
}

/// A Preview that shows a picture and its details, as after a good render.
fn with_picture() -> Preview {
    let mut preview = Preview::default();
    preview.set_meta(Some(Meta {
        name: "poster_v3.psd".into(),
        dimensions: None,
        size: None,
        format: "PSD".into(),
    }));
    preview.set_image(Some(PreviewImage {
        texture: TextureId::User(1),
        size: (64, 48),
    }));
    preview
}

/// Run one frame in a pane `size` wide and tall. Give the painted texts with
/// their positions, and every plain rectangle with its stroke.
fn frame(
    ctx: &Context,
    preview: &mut Preview,
    size: f32,
    events: Vec<Event>,
) -> (Texts, Vec<(CornerRadius, Stroke)>) {
    fn rects(shape: &Shape, found: &mut Vec<(CornerRadius, Stroke)>) {
        match shape {
            Shape::Vec(shapes) => shapes.iter().for_each(|s| rects(s, found)),
            Shape::Rect(r) if r.fill == tokens::BG_SECONDARY => {
                found.push((r.corner_radius, r.stroke));
            }
            _ => {}
        }
    }
    let input = egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(size, size))),
        events,
        ..Default::default()
    };
    let output = ctx.run_ui(input, |ui| preview.show(ui));
    let (mut texts, mut boxes) = (Vec::new(), Vec::new());
    for clipped in &output.shapes {
        collect(&clipped.shape, &mut texts);
        rects(&clipped.shape, &mut boxes);
    }
    output.drop_without_applying_deltas();
    (texts, boxes)
}

/// Run a few frames so the layout settles, and give the last one.
fn settle(ctx: &Context, preview: &mut Preview, size: f32) -> Vec<(String, Pos2)> {
    let mut texts = Vec::new();
    for _ in 0..3 {
        texts = frame(ctx, preview, size, vec![]).0;
    }
    texts
}

fn position(texts: &[(String, Pos2)], text: &str) -> Pos2 {
    texts
        .iter()
        .find(|(t, _)| t == text)
        .unwrap_or_else(|| panic!("{text:?} is painted: {texts:?}"))
        .1
}

fn click(at: Pos2) -> Vec<Event> {
    let button = |pressed| Event::PointerButton {
        pos: at,
        button: PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    vec![Event::PointerMoved(at), button(true), button(false)]
}

#[test]
fn the_card_shows_the_title_the_file_name_and_the_message() {
    let ctx = context();
    let mut preview = with_picture();
    preview.set_failure(Some(failure("unexpected end of file")));
    let texts = settle(&ctx, &mut preview, 400.0);
    let title = position(&texts, "Preview failed");
    let name = position(&texts, "poster_v3.psd");
    let message = position(&texts, "unexpected end of file");
    position(&texts, "!");
    position(&texts, crate::preview::failure::HINT);
    // The name sits under the title, and the message under both.
    assert!(title.y < name.y && name.y < message.y, "{texts:?}");
    let (_, boxes) = frame(&ctx, &mut preview, 400.0, vec![]);
    let card = (
        CornerRadius::same(tokens::RADIUS_L),
        Stroke::new(tokens::STROKE_WIDTH, tokens::CORAL_DARK),
    );
    assert!(boxes.contains(&card), "{boxes:?}");
}

#[test]
fn a_click_on_each_button_queues_one_event() {
    let ctx = context();
    let mut preview = with_picture();
    preview.set_failure(Some(failure("unexpected end of file")));
    let texts = settle(&ctx, &mut preview, 400.0);
    let retry = position(&texts, "Retry") + vec2(2.0, 2.0);
    let editor = position(&texts, "Open in editor") + vec2(2.0, 2.0);
    frame(&ctx, &mut preview, 400.0, click(retry));
    assert_eq!(preview.take_events(), vec![PreviewEvent::RetryRender]);
    assert_eq!(preview.take_events(), Vec::new());
    frame(&ctx, &mut preview, 400.0, click(editor));
    assert_eq!(preview.take_events(), vec![PreviewEvent::OpenInEditor]);
    assert_eq!(preview.take_events(), Vec::new());
}

#[test]
fn clearing_the_failure_brings_the_picture_box_back() {
    let ctx = context();
    let mut preview = with_picture();
    preview.set_failure(Some(failure("bad")));
    let texts = settle(&ctx, &mut preview, 400.0);
    position(&texts, "Preview failed");

    preview.set_failure(None);
    settle(&ctx, &mut preview, 400.0);
    let (texts, boxes) = frame(&ctx, &mut preview, 400.0, vec![]);
    assert!(
        texts.iter().all(|(t, _)| t != "Preview failed"),
        "{texts:?}"
    );
    let box_radius = CornerRadius::same(tokens::RADIUS_L);
    assert!(
        boxes
            .iter()
            .any(|(radius, stroke)| *radius == box_radius && stroke.color != tokens::CORAL_DARK),
        "the picture box is painted: {boxes:?}"
    );
}

#[test]
fn the_empty_hint_shows_without_a_selection_and_the_card_wins_over_it() {
    let ctx = context();
    let mut preview = Preview::default();
    position(&settle(&ctx, &mut preview, 400.0), EMPTY_HINT);
    preview.set_failure(Some(failure("bad")));
    let texts = settle(&ctx, &mut preview, 400.0);
    position(&texts, "Preview failed");
    assert!(texts.iter().all(|(t, _)| t != EMPTY_HINT), "{texts:?}");
    preview.set_failure(None);
    position(&settle(&ctx, &mut preview, 400.0), EMPTY_HINT);
}

#[test]
fn a_long_message_wraps_and_the_panel_scrolls_in_a_short_pane() {
    let ctx = context();
    let mut preview = with_picture();
    let message = "unexpected end of file while reading the layer stack ".repeat(12);
    preview.set_failure(Some(failure(&message)));
    let size = 200.0;
    let input = egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(size, size))),
        ..Default::default()
    };
    let mut lines = 0;
    let mut widest = 0.0_f32;
    let mut bottom = 0.0_f32;
    for _ in 0..3 {
        let output = ctx.run_ui(input.clone(), |ui| preview.show(ui));
        for clipped in &output.shapes {
            let mut stack = vec![&clipped.shape];
            while let Some(shape) = stack.pop() {
                match shape {
                    Shape::Vec(inner) => stack.extend(inner),
                    Shape::Text(t)
                        if t.galley.job.text == message.trim_end()
                            || t.galley.job.text == message =>
                    {
                        lines = t.galley.rows.len();
                        widest = t.galley.rect.width();
                    }
                    Shape::Text(t) => bottom = bottom.max(t.pos.y),
                    _ => {}
                }
            }
        }
        output.drop_without_applying_deltas();
    }
    assert!(lines > 1, "the message wraps: {lines}");
    assert!(widest <= size, "inside the pane: {widest}");
    assert!(bottom > size, "the buttons are below the fold: {bottom}");
}

#[test]
fn a_new_selection_clears_the_failure() {
    let ctx = context();
    let mut preview = with_picture();
    preview.set_failure(Some(failure("bad")));
    position(&settle(&ctx, &mut preview, 400.0), "Preview failed");
    preview.selection_changed();
    let texts = settle(&ctx, &mut preview, 400.0);
    assert!(
        texts.iter().all(|(t, _)| t != "Preview failed"),
        "{texts:?}"
    );
}
