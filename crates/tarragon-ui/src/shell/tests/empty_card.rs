//! Tests for the empty card button in a full Shell.

use super::super::{MenuEvent, Shell};
use crate::test_support;
use egui::epaint::Shape;
use egui::{Context, Event, Pos2, Rect};

/// Run one frame with `events`; return the centre of the "Open Folder..."
/// button label if it was painted.
fn frame(ctx: &Context, shell: &mut Shell, events: Vec<Event>) -> Option<Pos2> {
    fn find(shape: &Shape, found: &mut Option<Pos2>) {
        match shape {
            Shape::Vec(shapes) => shapes.iter().for_each(|s| find(s, found)),
            Shape::Text(t) if t.galley.job.text == "Open Folder..." => {
                // The File menu popup paints later; the last match is the card.
                *found = Some(t.galley.rect.translate(t.pos.to_vec2()).center());
            }
            _ => {}
        }
    }
    let mut input = test_support::input(1400.0, events);
    input.screen_rect = Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(1400.0, 900.0)));
    let output = ctx.run_ui(input, |ui| shell.show(ui));
    let mut found = None;
    for clipped in &output.shapes {
        find(&clipped.shape, &mut found);
    }
    output.drop_without_applying_deltas();
    found
}

/// A shell with no folder, drawn twice, and where its card button is.
fn setup() -> (Context, Shell, Pos2) {
    let ctx = test_support::context();
    let mut shell = Shell::new();
    frame(&ctx, &mut shell, vec![]);
    let at = frame(&ctx, &mut shell, vec![]).expect("the card button is drawn");
    (ctx, shell, at)
}

#[test]
fn a_click_on_the_card_button_queues_one_open_folder_event() {
    let (ctx, mut shell, at) = setup();
    frame(&ctx, &mut shell, test_support::click(at));
    assert_eq!(shell.take_menu_events(), vec![MenuEvent::OpenFolder]);
    assert_eq!(shell.take_menu_events(), vec![]);
}

#[test]
fn a_frame_without_a_click_queues_no_event() {
    let (ctx, mut shell, _) = setup();
    frame(&ctx, &mut shell, vec![]);
    assert_eq!(shell.take_menu_events(), vec![]);
}
