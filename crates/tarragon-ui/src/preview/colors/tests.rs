//! Tests for the colour bars.

use super::{ColorState, ColorStates};
use crate::preview::{Meta, Preview, PreviewEvent};
use crate::swatch::{Swatch, bucket_color};
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
        modified: None,
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

/// The rects of the colour bars. The checkerboard bar is cut into cells, so
/// it never matches a whole bar.
fn bar_rects(found: &[(Rect, egui::Color32)]) -> Vec<Rect> {
    found
        .iter()
        .filter(|(rect, _)| rect.size() == vec2(18.0, 8.0))
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
fn only_the_carried_colours_show_as_bars_with_the_opacity_of_their_state() {
    let ctx = Context::default();
    crate::fonts::install(&ctx);
    let mut preview = preview();
    preview.set_colors(states());
    let found = frame(&ctx, &mut preview, vec![]);
    let bars = bar_rects(&found);
    assert_eq!(bars.len(), 4, "one bar per carried colour: {found:?}");
    // One row, left to right, three points apart.
    for pair in bars.windows(2) {
        assert_eq!(pair[0].min.y, pair[1].min.y);
        assert!((pair[1].min.x - pair[0].max.x - 3.0).abs() < 0.01);
    }
    for (bar, index) in bars.iter().zip([0, 1, 5, 6]) {
        let Swatch::Bucket(bucket) = Swatch::ALL[index] else {
            panic!("a bucket");
        };
        let want = bucket_color(bucket).gamma_multiply(states()[index].opacity());
        let fill = found.iter().find(|(rect, _)| rect == bar).map(|(_, f)| *f);
        assert_eq!(fill, Some(want), "bar of swatch {index}");
    }
}

#[test]
fn a_transparent_image_shows_the_checker_bar() {
    let ctx = Context::default();
    crate::fonts::install(&ctx);
    let mut preview = preview();
    let mut states = ColorStates::default();
    states[11] = ColorState::All;
    preview.set_colors(states);
    let found = frame(&ctx, &mut preview, vec![]);
    assert!(found.iter().any(|(_, fill)| *fill == tokens::CHECKER_LIGHT));
    assert!(found.iter().any(|(_, fill)| *fill == tokens::CHECKER_DARK));
}

#[test]
fn the_three_states_give_the_mockup_opacities() {
    assert_eq!(ColorState::All.opacity(), 1.0);
    assert_eq!(ColorState::Some.opacity(), 0.5);
    assert_eq!(ColorState::None.opacity(), 0.3);
}

#[test]
fn no_bar_shows_when_colour_tags_are_off_or_no_colour_is_carried() {
    let ctx = Context::default();
    crate::fonts::install(&ctx);
    let mut preview = preview();
    let found = frame(&ctx, &mut preview, vec![]);
    assert!(bar_rects(&found).is_empty(), "no colour, no bar");

    preview.set_colors(states());
    preview.set_colors_enabled(false);
    let found = frame(&ctx, &mut preview, vec![]);
    assert!(bar_rects(&found).is_empty(), "{found:?}");

    preview.set_colors_enabled(true);
    let found = frame(&ctx, &mut preview, vec![]);
    assert_eq!(bar_rects(&found).len(), 4);
}

#[test]
fn a_click_on_a_bar_queues_no_event() {
    let ctx = Context::default();
    crate::fonts::install(&ctx);
    let mut preview = preview();
    preview.set_colors(states());
    let bars = bar_rects(&frame(&ctx, &mut preview, vec![]));
    let at = bars[0].center();
    let button = |pressed| Event::PointerButton {
        pos: at,
        button: PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    frame(
        &ctx,
        &mut preview,
        vec![Event::PointerMoved(at), button(true), button(false)],
    );
    assert_eq!(preview.take_events(), Vec::<PreviewEvent>::new());
}
