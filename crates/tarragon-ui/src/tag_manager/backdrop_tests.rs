//! Tests for the dim layer: it paints, takes clicks, and shields the panes.

use super::TagManager;
use super::tests::{escape, row};
use crate::{test_support, tokens};
use egui::epaint::Shape;
use egui::{Context, Event, Pos2, Rect, vec2};

/// A point on the screen that the centred window does not cover.
const CORNER: Pos2 = Pos2::new(12.0, 12.0);

/// Run one frame with a pane button at the top-left, then the window. Return
/// the pane button response and the shapes drawn.
fn pane_frame(
    ctx: &Context,
    manager: &mut TagManager,
    events: Vec<Event>,
) -> (bool, Vec<egui::epaint::ClippedShape>) {
    let mut clicked = false;
    let output = ctx.run_ui(test_support::input(600.0, events), |ui| {
        clicked = ui.button("pane").clicked();
        manager.show(ui.ctx());
    });
    let shapes = output.shapes.clone();
    output.drop_without_applying_deltas();
    (clicked, shapes)
}

fn open_manager() -> (Context, TagManager) {
    let ctx = test_support::context();
    let mut manager = TagManager::default();
    manager.set_rows(vec![row(1, "sky", 2)]);
    manager.open();
    // The pane must be drawn before the click, so its rect is known.
    pane_frame(&ctx, &mut manager, vec![]);
    pane_frame(&ctx, &mut manager, vec![]);
    (ctx, manager)
}

fn fills(shape: &Shape, found: &mut Vec<(Rect, egui::Color32)>) {
    match shape {
        Shape::Vec(shapes) => shapes.iter().for_each(|s| fills(s, found)),
        Shape::Rect(r) => found.push((r.rect, r.fill)),
        _ => {}
    }
}

#[test]
fn the_dim_colour_covers_the_whole_screen() {
    let (ctx, mut manager) = open_manager();
    let (_, shapes) = pane_frame(&ctx, &mut manager, vec![]);
    let mut found = Vec::new();
    for clipped in &shapes {
        fills(&clipped.shape, &mut found);
    }
    let screen = Rect::from_min_size(Pos2::ZERO, vec2(600.0, 400.0));
    assert!(
        found
            .iter()
            .any(|(rect, fill)| *fill == tokens::SCRIM && rect.contains_rect(screen)),
        "no full-screen dim rect in {found:?}"
    );
}

#[test]
fn a_click_on_the_dim_layer_keeps_the_window_open() {
    let (ctx, mut manager) = open_manager();
    pane_frame(&ctx, &mut manager, test_support::click(CORNER));
    pane_frame(&ctx, &mut manager, vec![]);
    assert!(manager.is_open());
}

#[test]
fn a_click_does_not_reach_a_pane_while_the_window_is_open() {
    let (ctx, mut manager) = open_manager();
    let at = CORNER + vec2(4.0, 4.0);
    let (hit, _) = pane_frame(&ctx, &mut manager, test_support::click(at));
    assert!(!hit, "the pane took a click through the dim layer");

    // Control: the same click lands once the window is closed.
    manager.close();
    pane_frame(&ctx, &mut manager, vec![]);
    let (hit, _) = pane_frame(&ctx, &mut manager, test_support::click(at));
    assert!(hit, "the pane button is not at the clicked point");
}

/// Run one frame with a pane button, then the window. Return whether the pane
/// button has keyboard focus.
fn pane_focused(ctx: &Context, manager: &mut TagManager, events: Vec<Event>) -> bool {
    let mut id = None;
    let output = ctx.run_ui(test_support::input(600.0, events), |ui| {
        id = Some(ui.button("pane").id);
        manager.show(ui.ctx());
    });
    output.drop_without_applying_deltas();
    ctx.memory(|m| m.has_focus(id.expect("the pane button")))
}

fn tab() -> Event {
    Event::Key {
        key: egui::Key::Tab,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }
}

#[test]
fn tab_does_not_move_focus_to_a_pane_while_the_window_is_open() {
    let (ctx, mut manager) = open_manager();
    for _ in 0..3 {
        assert!(!pane_focused(&ctx, &mut manager, vec![tab()]));
    }

    // Control: with the window closed, Tab reaches the pane button.
    manager.close();
    pane_focused(&ctx, &mut manager, vec![]);
    assert!(pane_focused(&ctx, &mut manager, vec![tab()]));
}

#[test]
fn escape_still_closes_over_the_dim_layer() {
    let (ctx, mut manager) = open_manager();
    pane_frame(&ctx, &mut manager, vec![escape()]);
    assert!(!manager.is_open());
}
