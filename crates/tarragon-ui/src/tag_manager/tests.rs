//! Tests for the "Manage tags" window.

use super::{TagManager, TagManagerEvent, TagRow};
use egui::epaint::Shape;
use egui::{Context, Event, Key, Modifiers, Order, Pos2};

pub(super) fn row(id: i64, name: &str, count: u32) -> TagRow {
    TagRow {
        id,
        name: name.to_owned(),
        count,
    }
}

/// Run one frame; return every painted text.
pub(super) fn frame(ctx: &Context, manager: &mut TagManager, events: Vec<Event>) -> Vec<String> {
    frame_at(ctx, manager, events)
        .into_iter()
        .map(|(text, _)| text)
        .collect()
}

/// Run one frame; return every painted text with its position.
pub(super) fn frame_at(
    ctx: &Context,
    manager: &mut TagManager,
    events: Vec<Event>,
) -> Vec<(String, Pos2)> {
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
pub(super) fn settled(ctx: &Context, manager: &mut TagManager) -> Vec<String> {
    frame(ctx, manager, vec![]);
    frame(ctx, manager, vec![])
}

fn collect(shape: &Shape, found: &mut Vec<(String, Pos2)>) {
    match shape {
        Shape::Vec(shapes) => shapes.iter().for_each(|s| collect(s, found)),
        Shape::Text(t) => found.push((t.galley.job.text.clone(), t.pos)),
        _ => {}
    }
}

pub(super) fn escape() -> Event {
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

fn key(key: Key, modifiers: Modifiers) -> Event {
    Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers,
    }
}

/// An open window with `rows`, with the Rename button of the first row clicked
/// and the field focused.
pub(super) fn editing(rows: Vec<TagRow>) -> (Context, TagManager) {
    let ctx = crate::test_support::context();
    let mut manager = TagManager::default();
    manager.set_rows(rows);
    manager.open();
    frame(&ctx, &mut manager, vec![]);
    let shown = frame_at(&ctx, &mut manager, vec![]);
    let (_, at) = shown
        .iter()
        .find(|(text, _)| text == "Rename")
        .expect("a Rename button");
    frame(
        &ctx,
        &mut manager,
        crate::test_support::click(*at + egui::vec2(4.0, 4.0)),
    );
    frame(&ctx, &mut manager, vec![]);
    frame(&ctx, &mut manager, vec![]);
    (ctx, manager)
}

/// Replace the field text with `text`, press Enter, and return the texts of
/// the frame after.
fn submit(ctx: &Context, manager: &mut TagManager, text: &str) -> Vec<String> {
    let select_all = key(Key::A, Modifiers::COMMAND);
    let replace = if text.is_empty() {
        key(Key::Backspace, Modifiers::NONE)
    } else {
        Event::Text(text.to_owned())
    };
    frame(ctx, manager, vec![select_all, replace]);
    frame(ctx, manager, vec![key(Key::Enter, Modifiers::NONE)]);
    frame(ctx, manager, vec![])
}

fn rename(id: i64, new_name: &str) -> TagManagerEvent {
    TagManagerEvent::Rename {
        id,
        new_name: new_name.to_owned(),
    }
}

#[test]
fn enter_queues_the_trimmed_name_and_ends_the_edit() {
    let (ctx, mut manager) = editing(vec![row(1, "a", 1)]);
    let texts = submit(&ctx, &mut manager, " b ");
    assert_eq!(manager.take_events(), vec![rename(1, "b")]);
    assert_eq!(manager.take_events(), vec![]);
    assert!(texts.iter().any(|t| t == "Rename"), "edit did not end");
}

#[test]
fn refused_names_show_the_message_and_queue_nothing() {
    for (typed, message) in [
        ("", "Enter a tag name"),
        (&"a".repeat(65), "Name is longer than 64 characters"),
        ("Red", "Name is reserved"),
        ("transparent", "Name is reserved"),
        ("b", "name already used"),
    ] {
        let (ctx, mut manager) = editing(vec![row(1, "a", 1), row(2, "b", 1)]);
        let texts = submit(&ctx, &mut manager, typed);
        assert!(texts.iter().any(|t| t == message), "{typed:?}: {texts:?}");
        assert_eq!(manager.take_events(), vec![], "{typed:?} queued");
    }
}

#[test]
fn names_differing_only_in_case_are_allowed() {
    let (ctx, mut manager) = editing(vec![row(1, "Cat", 1), row(2, "cat", 1)]);
    submit(&ctx, &mut manager, "CAT");
    assert_eq!(manager.take_events(), vec![rename(1, "CAT")]);
    let (ctx, mut manager) = editing(vec![row(1, "cat", 1), row(2, "dog", 1)]);
    submit(&ctx, &mut manager, "Cat");
    assert_eq!(manager.take_events(), vec![rename(1, "Cat")]);
}

#[test]
fn the_same_name_queues_nothing_and_ends_the_edit() {
    let (ctx, mut manager) = editing(vec![row(1, "a", 1)]);
    let texts = submit(&ctx, &mut manager, " a ");
    assert_eq!(manager.take_events(), vec![]);
    assert!(texts.iter().any(|t| t == "Rename"), "edit did not end");
}

#[test]
fn first_escape_ends_the_edit_and_the_second_closes_the_window() {
    let (ctx, mut manager) = editing(vec![row(1, "a", 1)]);
    let texts = frame(&ctx, &mut manager, vec![escape()]);
    assert!(manager.is_open());
    assert_eq!(manager.take_events(), vec![]);
    assert!(texts.iter().any(|t| t == "Rename"), "edit did not end");

    frame(&ctx, &mut manager, vec![escape()]);
    assert!(!manager.is_open());
}

#[test]
fn set_error_reopens_the_edit_with_the_text() {
    let ctx = crate::test_support::context();
    let mut manager = TagManager::default();
    manager.set_rows(vec![row(1, "a", 1)]);
    manager.open();
    manager.set_error(1, "refused".to_owned());
    frame(&ctx, &mut manager, vec![]);
    let texts = frame(&ctx, &mut manager, vec![]);
    assert!(texts.iter().any(|t| t == "refused"));
    assert!(!texts.iter().any(|t| t == "Rename"));
}

#[test]
fn the_x_button_closes_the_window() {
    let ctx = crate::test_support::context();
    let mut manager = TagManager::default();
    manager.set_rows(vec![row(1, "sky", 2)]);
    manager.open();
    settled(&ctx, &mut manager);
    let area = ctx
        .memory(|m| {
            let layers = m.areas().visible_layer_ids();
            let window = layers.into_iter().find(|l| l.order == Order::Middle)?;
            m.area_rect(window.id)
        })
        .expect("the window area");
    let at = area.right_top() + egui::vec2(-12.0, 12.0);
    frame(&ctx, &mut manager, crate::test_support::click(at));
    assert!(!manager.is_open());
}

pub(super) fn count(texts: &[String], want: &str) -> usize {
    texts.iter().filter(|t| *t == want).count()
}

#[test]
fn an_edit_ends_when_set_rows_drops_its_row() {
    let (ctx, mut manager) = editing(vec![row(1, "a", 1), row(2, "b", 1)]);
    manager.set_rows(vec![row(2, "b", 1)]);
    frame(&ctx, &mut manager, vec![]);
    manager.set_rows(vec![row(1, "a", 1), row(2, "b", 1)]);
    let texts = settled(&ctx, &mut manager);
    assert_eq!(count(&texts, "Rename"), 2, "the old edit came back");
}

#[test]
fn close_ends_the_edit() {
    let (ctx, mut manager) = editing(vec![row(1, "a", 1)]);
    manager.close();
    manager.open();
    let texts = settled(&ctx, &mut manager);
    assert_eq!(count(&texts, "Rename"), 1, "the edit survived close");
}

#[test]
fn a_second_rename_click_moves_the_edit_to_that_row() {
    let (ctx, mut manager) = editing(vec![row(1, "a", 1), row(2, "b", 1)]);
    let shown = frame_at(&ctx, &mut manager, vec![]);
    let (_, at) = shown
        .iter()
        .find(|(text, _)| text == "Rename")
        .expect("the second Rename button");
    frame(
        &ctx,
        &mut manager,
        crate::test_support::click(*at + egui::vec2(4.0, 4.0)),
    );
    frame(&ctx, &mut manager, vec![]);
    let texts = submit(&ctx, &mut manager, "z");
    assert_eq!(manager.take_events(), vec![rename(2, "z")]);
    assert_eq!(count(&texts, "Rename"), 2);
}

#[test]
fn set_error_refills_the_name_that_was_refused() {
    let (ctx, mut manager) = editing(vec![row(1, "a", 1)]);
    submit(&ctx, &mut manager, "b");
    manager.set_rows(vec![row(1, "a", 1)]);
    manager.set_error(1, "refused".to_owned());
    frame(&ctx, &mut manager, vec![]);
    let texts = frame(&ctx, &mut manager, vec![]);
    assert!(texts.iter().any(|t| t == "b"), "{texts:?}");
    assert!(!texts.iter().any(|t| t == "a"), "{texts:?}");
}
