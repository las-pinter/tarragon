//! Tests for the refusal message in the name input.

use super::{AddState, Preview, PreviewEvent, frame, has, key, open_input, preview_with, setup};
use crate::tokens;
use egui::{Color32, Context, Event, Key, Pos2, Rect, Shape, vec2};
use tarragon_core::tag_name::TagNameError;

const HINT: &str = "Press Enter to add. Escape or focus loss cancels.";

/// An open input with `typed` entered and Enter pressed once.
fn entered(known: &[&str], typed: &str) -> (Context, Preview) {
    let mut preview = preview_with(vec![], known);
    let ctx = setup(&mut preview);
    open_input(&ctx, &mut preview);
    frame(&ctx, &mut preview, vec![Event::Text(typed.into())]);
    frame(&ctx, &mut preview, vec![key(Key::Enter)]);
    (ctx, preview)
}

/// The text and the error of the open input.
fn input(preview: &Preview) -> Option<(&str, Option<TagNameError>)> {
    match &preview.add {
        AddState::Input { text, error, .. } => Some((text, *error)),
        _ => None,
    }
}

#[test]
fn reserved_names_show_the_reserved_message_and_queue_nothing() {
    for name in ["red", "Transparent"] {
        let (ctx, mut preview) = entered(&[], name);
        let texts = frame(&ctx, &mut preview, vec![]);
        assert!(has(&texts, "Name is reserved"), "{name}");
        assert!(!has(&texts, HINT), "the message replaces the hint");
        assert_eq!(preview.take_events(), Vec::<PreviewEvent>::new(), "{name}");
    }
}

#[test]
fn a_long_name_shows_the_length_message() {
    let (ctx, mut preview) = entered(&[], &"a".repeat(65));
    let texts = frame(&ctx, &mut preview, vec![]);
    assert!(has(&texts, "Name is longer than 64 characters"));
    assert_eq!(preview.take_events(), Vec::<PreviewEvent>::new());
}

#[test]
fn an_empty_name_shows_the_enter_a_name_message() {
    let (ctx, mut preview) = entered(&[], "");
    assert!(has(&frame(&ctx, &mut preview, vec![]), "Enter a tag name"));
    assert_eq!(preview.take_events(), Vec::<PreviewEvent>::new());
}

#[test]
fn after_a_refusal_the_input_keeps_focus_and_typing_clears_the_message() {
    let (ctx, mut preview) = entered(&[], "red");
    frame(&ctx, &mut preview, vec![]);
    assert_eq!(input(&preview), Some(("red", Some(TagNameError::Reserved))));
    // Text only reaches a focused field.
    frame(&ctx, &mut preview, vec![Event::Text("x".into())]);
    assert_eq!(input(&preview), Some(("redx", None)));
    let texts = frame(&ctx, &mut preview, vec![]);
    assert!(has(&texts, HINT) && !has(&texts, "Name is reserved"));
}

#[test]
fn a_second_refusal_shows_the_new_reason() {
    let (ctx, mut preview) = entered(&[], "red");
    frame(&ctx, &mut preview, vec![]);
    for _ in 0..3 {
        frame(&ctx, &mut preview, vec![key(Key::Backspace)]);
    }
    frame(&ctx, &mut preview, vec![key(Key::Enter)]);
    assert_eq!(input(&preview), Some(("", Some(TagNameError::Empty))));
}

/// The first-section colour of the painted text `want`, from one frame.
fn colour_of(ctx: &Context, preview: &mut Preview, want: &str) -> Option<Color32> {
    fn walk(shape: &Shape, want: &str, found: &mut Option<Color32>) {
        match shape {
            Shape::Vec(shapes) => shapes.iter().for_each(|s| walk(s, want, found)),
            Shape::Text(t) if t.galley.job.text == want => {
                *found = Some(t.galley.job.sections[0].format.color);
            }
            _ => {}
        }
    }
    let input = egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(400.0, 600.0))),
        ..Default::default()
    };
    let output = ctx.run_ui(input, |ui| preview.show(ui));
    let mut found = None;
    output
        .shapes
        .iter()
        .for_each(|c| walk(&c.shape, want, &mut found));
    output.drop_without_applying_deltas();
    found
}

#[test]
fn the_refusal_is_painted_in_the_error_colour_and_the_hint_in_the_muted_colour() {
    let mut preview = preview_with(vec![], &[]);
    let ctx = setup(&mut preview);
    open_input(&ctx, &mut preview);
    assert_eq!(
        colour_of(&ctx, &mut preview, HINT),
        Some(tokens::TEXT_MUTED)
    );
    frame(&ctx, &mut preview, vec![Event::Text("red".into())]);
    frame(&ctx, &mut preview, vec![key(Key::Enter)]);
    assert_eq!(
        colour_of(&ctx, &mut preview, "Name is reserved"),
        Some(tokens::TAG_ERROR_TEXT)
    );
}

#[test]
fn a_known_tag_name_is_valid_and_applies_that_tag() {
    let (ctx, mut preview) = entered(&["hands"], "hands");
    assert_eq!(
        preview.take_events(),
        [PreviewEvent::TagApplied("hands".into())]
    );
    assert!(has(&frame(&ctx, &mut preview, vec![]), "+ add"));
}

#[test]
fn one_escape_after_a_refusal_closes_the_input_with_no_event() {
    let (ctx, mut preview) = entered(&[], "red");
    frame(&ctx, &mut preview, vec![]);
    frame(&ctx, &mut preview, vec![key(Key::Escape)]);
    let texts = frame(&ctx, &mut preview, vec![]);
    assert!(has(&texts, "+ add") && !has(&texts, "Name is reserved"));
    assert_eq!(input(&preview), None);
    assert_eq!(preview.take_events(), Vec::<PreviewEvent>::new());
}

#[test]
fn focus_loss_after_a_refusal_closes_the_input_with_no_event() {
    let (ctx, mut preview) = entered(&[], "red");
    frame(&ctx, &mut preview, vec![]);
    frame(&ctx, &mut preview, super::click(Pos2::new(380.0, 580.0)));
    assert!(has(&frame(&ctx, &mut preview, vec![]), "+ add"));
    assert_eq!(preview.take_events(), Vec::<PreviewEvent>::new());
}

#[test]
fn a_reopened_input_starts_with_no_message() {
    let (ctx, mut preview) = entered(&[], "red");
    frame(&ctx, &mut preview, vec![key(Key::Escape)]);
    open_input(&ctx, &mut preview);
    assert_eq!(input(&preview), Some(("", None)));
    assert!(has(&frame(&ctx, &mut preview, vec![]), HINT));
}
