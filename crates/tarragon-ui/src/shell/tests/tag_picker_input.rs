//! Tests that the name input of the tag picker keeps its keys from the grid.

use super::super::Shell;
use super::grid_keys::path;
use super::keys::{frame, press, select_one, setup};
use crate::gallery::GridMenuEvent;
use crate::test_support;
use egui::{Context, Event, Key, Modifiers, Pos2, Rect, Shape, pos2, vec2};

/// Run one frame with `events`. Return every painted text.
fn frame_texts(ctx: &Context, shell: &mut Shell, events: Vec<Event>) -> Vec<(String, Pos2)> {
    fn collect(shape: &Shape, texts: &mut Vec<(String, Pos2)>) {
        match shape {
            Shape::Vec(shapes) => shapes.iter().for_each(|shape| collect(shape, texts)),
            Shape::Text(text) => texts.push((text.galley.job.text.clone(), text.pos)),
            _ => {}
        }
    }
    let mut input = test_support::input(1400.0, events);
    input.screen_rect = Some(Rect::from_min_size(Pos2::ZERO, vec2(1400.0, 900.0)));
    let output = ctx.run_ui(input, |ui| shell.show(ui));
    let mut texts = Vec::new();
    for clipped in &output.shapes {
        collect(&clipped.shape, &mut texts);
    }
    output.drop_without_applying_deltas();
    texts
}

/// Select the second tile and open the picker. Click "Create new...", and
/// stop on the first frame that draws the input.
fn open_fresh_input(ctx: &Context, shell: &mut Shell) {
    select_one(ctx, shell, 1);
    shell
        .gallery_mut()
        .open_tag_picker(pos2(300.0, 300.0), vec![path("b.png")]);
    frame(ctx, shell, vec![]);
    let at = frame_texts(ctx, shell, vec![])
        .into_iter()
        .find_map(|(text, at)| (text == "Create new...").then_some(at))
        .expect("the menu was drawn")
        + vec2(3.0, 3.0);
    for events in test_support::click(at) {
        frame_texts(ctx, shell, vec![events]);
    }
}

#[test]
fn delete_in_the_fresh_input_raises_no_grid_event() {
    let (ctx, mut shell) = setup();
    open_fresh_input(&ctx, &mut shell);
    // The input is not focused yet in this frame, so only the window gate
    // keeps the key from the grid.
    frame(&ctx, &mut shell, vec![press(Key::Delete, Modifiers::NONE)]);
    assert_eq!(shell.gallery_mut().take_menu_events(), vec![]);
    assert!(shell.gallery_mut().tag_picker_open(), "the input is open");
}

#[test]
fn typing_in_the_input_edits_the_text_and_raises_no_grid_event() {
    let (ctx, mut shell) = setup();
    open_fresh_input(&ctx, &mut shell);
    for _ in 0..3 {
        frame(&ctx, &mut shell, vec![]);
    }
    frame_texts(&ctx, &mut shell, vec![Event::Text("ab".to_owned())]);
    frame(&ctx, &mut shell, vec![press(Key::Home, Modifiers::NONE)]);
    frame(&ctx, &mut shell, vec![press(Key::Delete, Modifiers::NONE)]);
    let seen = frame_texts(&ctx, &mut shell, vec![]);
    assert!(seen.iter().any(|(text, _)| text == "b"), "Delete took a");
    assert!(!seen.iter().any(|(text, _)| text == "ab"));
    assert_eq!(shell.gallery_mut().take_menu_events(), vec![]);

    frame(&ctx, &mut shell, vec![press(Key::Delete, Modifiers::SHIFT)]);
    let events = shell.gallery_mut().take_menu_events();
    assert!(
        !events.iter().any(|event| matches!(
            event,
            GridMenuEvent::Delete(_) | GridMenuEvent::DeletePermanently(_)
        )),
        "{events:?}"
    );
}
