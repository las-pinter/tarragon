//! Tests for the colour square row.

use super::{ColorState, ColorStates};
use crate::preview::{Meta, Preview, PreviewEvent};
use crate::swatch::{self, Swatch, bucket_color};
use crate::tokens;
use egui::{Context, Event, PointerButton, Pos2, Rect, vec2};

fn input(events: Vec<Event>) -> egui::RawInput {
    egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(500.0, 600.0))),
        events,
        ..Default::default()
    }
}

fn preview() -> Preview {
    let mut preview = Preview::default();
    preview.set_meta(Some(Meta {
        name: "a.png".into(),
        dimensions: None,
        size: None,
        format: "PNG".into(),
    }));
    preview
}

/// Every plain rectangle painted in one frame, with its fill.
fn frame(ctx: &Context, preview: &mut Preview, events: Vec<Event>) -> Vec<(Rect, egui::Color32)> {
    use egui::epaint::Shape;
    fn collect(shape: &Shape, found: &mut Vec<(Rect, egui::Color32)>) {
        match shape {
            Shape::Vec(shapes) => shapes.iter().for_each(|s| collect(s, found)),
            Shape::Rect(r) => found.push((r.rect, r.fill)),
            _ => {}
        }
    }
    let output = ctx.run_ui(input(events), |ui| preview.show(ui));
    let mut found = Vec::new();
    for clipped in &output.shapes {
        collect(&clipped.shape, &mut found);
    }
    output.drop_without_applying_deltas();
    found
}

/// The rects of the 11 plain squares. The checkerboard is cut into cells, so
/// it never matches a full square.
fn square_rects(found: &[(Rect, egui::Color32)]) -> Vec<Rect> {
    found
        .iter()
        .filter(|(rect, _)| rect.size() == vec2(swatch::SIZE, swatch::SIZE))
        .map(|(rect, _)| *rect)
        .collect()
}

fn states() -> ColorStates {
    let mut states = ColorStates::default();
    states[0] = ColorState::All;
    states[1] = ColorState::Some;
    states[5] = ColorState::Some;
    states[6] = ColorState::All;
    states
}

#[test]
fn the_squares_are_painted_in_swatch_order_with_the_opacity_of_their_state() {
    let ctx = Context::default();
    let mut preview = preview();
    preview.set_colors(states());
    let found = frame(&ctx, &mut preview, vec![]);
    let squares = square_rects(&found);
    assert_eq!(squares.len(), 11, "the bucket squares: {found:?}");
    // They sit on one row, left to right, spaced by the extra-small gap.
    for pair in squares.windows(2) {
        assert_eq!(pair[0].min.y, pair[1].min.y);
        assert!((pair[1].min.x - pair[0].max.x - tokens::SPACING_XS).abs() < 0.01);
    }
    for (i, swatch) in Swatch::ALL.iter().enumerate() {
        let opacity = states()[i].opacity();
        match swatch {
            Swatch::Bucket(bucket) => {
                let want = bucket_color(*bucket).gamma_multiply(opacity);
                let at = found.iter().find(|(rect, _)| *rect == squares[i]);
                assert_eq!(at.map(|(_, fill)| *fill), Some(want), "square {i}");
            }
            Swatch::Transparent => {
                // The checkerboard starts one gap after the last bucket square.
                let first = squares[10].max.x + tokens::SPACING_XS;
                let cell = found
                    .iter()
                    .find(|(rect, _)| {
                        (rect.min.x - first).abs() < 0.01
                            && (rect.min.y - squares[0].min.y).abs() < 0.01
                    })
                    .expect("the first checker cell");
                let want = tokens::CHECKER_LIGHT.gamma_multiply(opacity);
                assert_eq!(cell.1, want, "transparent square");
            }
        }
    }
}

#[test]
fn the_three_states_give_the_mockup_opacities() {
    assert_eq!(ColorState::All.opacity(), 1.0);
    assert_eq!(ColorState::Some.opacity(), 0.5);
    assert_eq!(ColorState::None.opacity(), 0.3);
}

#[test]
fn no_square_is_painted_when_colour_tags_are_disabled() {
    let ctx = Context::default();
    let mut preview = preview();
    preview.set_colors(states());
    preview.set_colors_enabled(false);
    let found = frame(&ctx, &mut preview, vec![]);
    assert!(square_rects(&found).is_empty(), "{found:?}");
    assert!(
        !found
            .iter()
            .any(|(_, fill)| *fill == tokens::CHECKER_LIGHT.gamma_multiply(0.3)),
        "no checker cell either"
    );

    preview.set_colors_enabled(true);
    let found = frame(&ctx, &mut preview, vec![]);
    assert_eq!(square_rects(&found).len(), 11);
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
fn a_click_on_a_square_queues_one_event_naming_that_swatch() {
    let ctx = Context::default();
    let mut preview = preview();
    let squares = square_rects(&frame(&ctx, &mut preview, vec![]));
    assert_eq!(preview.take_events(), Vec::<PreviewEvent>::new());

    frame(&ctx, &mut preview, click(squares[6].center()));
    assert_eq!(
        preview.take_events(),
        [PreviewEvent::ColorClicked(Swatch::ALL[6])]
    );
    assert_eq!(
        preview.take_events(),
        Vec::<PreviewEvent>::new(),
        "draining empties the queue"
    );

    // The transparent square is the one after the last bucket square.
    let after = squares[10].max.x + tokens::SPACING_XS + swatch::SIZE / 2.0;
    frame(
        &ctx,
        &mut preview,
        click(Pos2::new(after, squares[10].center().y)),
    );
    assert_eq!(
        preview.take_events(),
        [PreviewEvent::ColorClicked(Swatch::Transparent)]
    );
}
