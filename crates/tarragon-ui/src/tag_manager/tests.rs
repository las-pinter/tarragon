//! Tests for the "Manage tags" window.

use super::{TagManager, TagRow};
use egui::epaint::Shape;
use egui::{Context, Event, Key, Modifiers};

fn row(id: i64, name: &str, count: u32) -> TagRow {
    TagRow {
        id,
        name: name.to_owned(),
        count,
    }
}

/// Run one frame; return every painted text.
fn frame(ctx: &Context, manager: &mut TagManager, events: Vec<Event>) -> Vec<String> {
    let output = ctx.run_ui(crate::test_support::input(600.0, events), |ui| {
        manager.show(ui.ctx());
    });
    let mut found = Vec::new();
    for clipped in &output.shapes {
        collect(&clipped.shape, &mut found);
    }
    output.drop_without_applying_deltas();
    found
}

/// Run two frames and return the texts of the second. An egui window is
/// hidden in its first frame, while it measures its content.
fn settled(ctx: &Context, manager: &mut TagManager) -> Vec<String> {
    frame(ctx, manager, vec![]);
    frame(ctx, manager, vec![])
}

fn collect(shape: &Shape, found: &mut Vec<String>) {
    match shape {
        Shape::Vec(shapes) => shapes.iter().for_each(|s| collect(s, found)),
        Shape::Text(t) => found.push(t.galley.job.text.clone()),
        _ => {}
    }
}

fn escape() -> Event {
    Event::Key {
        key: Key::Escape,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::NONE,
    }
}

#[test]
fn closed_manager_draws_nothing_and_open_shows_the_window() {
    let ctx = crate::test_support::context();
    let mut manager = TagManager::default();
    manager.set_rows(vec![row(1, "sky", 3)]);
    let texts = settled(&ctx, &mut manager);
    assert!(texts.is_empty(), "closed manager drew {texts:?}");

    manager.open();
    let texts = settled(&ctx, &mut manager);
    assert!(texts.iter().any(|t| t == "Manage tags"));
}

#[test]
fn rows_keep_their_order_and_one_reads_singular() {
    let ctx = crate::test_support::context();
    let mut manager = TagManager::default();
    manager.set_rows(vec![row(2, "zebra", 1), row(1, "apple", 12)]);
    manager.open();
    let texts = settled(&ctx, &mut manager);
    let at = |want: &str| texts.iter().position(|t| t == want);
    let (zebra, one) = (at("zebra"), at("1 image"));
    let (apple, twelve) = (at("apple"), at("12 images"));
    assert!(zebra.is_some() && one.is_some() && apple.is_some() && twelve.is_some());
    assert!(zebra < apple);
    assert!(one < twelve);
}

#[test]
fn empty_rows_show_the_empty_text() {
    let ctx = crate::test_support::context();
    let mut manager = TagManager::default();
    manager.open();
    let texts = settled(&ctx, &mut manager);
    assert!(texts.iter().any(|t| t == "No user tags yet"));
}

#[test]
fn escape_closes_and_set_rows_keeps_it_open() {
    let ctx = crate::test_support::context();
    let mut manager = TagManager::default();
    manager.open();
    frame(&ctx, &mut manager, vec![]);
    manager.set_rows(vec![row(1, "sky", 2)]);
    assert!(manager.is_open());
    frame(&ctx, &mut manager, vec![]);
    assert!(manager.is_open());

    frame(&ctx, &mut manager, vec![escape()]);
    assert!(!manager.is_open());
}
