//! Tests for the delete question in the "Manage tags" window.

use super::tests::{count, editing, escape, frame, frame_at, row, settled};
use super::{TagManager, TagManagerEvent, TagRow};
use egui::{Context, Order};

const QUESTION: &str = "Delete 'x' from 12 images?";

/// Click the button painted as `label`; `last` picks the lowest one, else the
/// highest. Return the texts of the frame after.
fn press(ctx: &Context, manager: &mut TagManager, label: &str, last: bool) -> Vec<String> {
    let shown = frame_at(ctx, manager, vec![]);
    let mut found = shown.iter().filter(|(text, _)| text == label);
    let (_, at) = if last {
        found.next_back()
    } else {
        found.next()
    }
    .unwrap_or_else(|| panic!("a {label} button"));
    let events = crate::test_support::click(*at + egui::vec2(4.0, 4.0));
    frame(ctx, manager, events);
    frame(ctx, manager, vec![])
}

fn open_with(rows: Vec<TagRow>) -> (Context, TagManager) {
    let ctx = crate::test_support::context();
    let mut manager = TagManager::default();
    manager.set_rows(rows);
    manager.open();
    settled(&ctx, &mut manager);
    (ctx, manager)
}

/// A window with one row "x" of 12 images, asking to delete it.
fn asking() -> (Context, TagManager) {
    let (ctx, mut manager) = open_with(vec![row(1, "x", 12)]);
    let texts = press(&ctx, &mut manager, "Delete", false);
    assert!(texts.iter().any(|t| t == QUESTION), "{texts:?}");
    (ctx, manager)
}

fn has(texts: &[String], want: &str) -> bool {
    texts.iter().any(|t| t == want)
}

#[test]
fn delete_click_shows_the_question_and_queues_nothing() {
    let (ctx, mut manager) = asking();
    let texts = frame(&ctx, &mut manager, vec![]);
    assert!(has(&texts, "Cancel"), "{texts:?}");
    assert_eq!(
        count(&texts, "Delete"),
        1,
        "the Delete button of the row stayed"
    );
    assert_eq!(manager.take_events(), vec![]);
}

#[test]
fn confirm_queues_one_delete_and_the_row_stays() {
    let (ctx, mut manager) = asking();
    let texts = press(&ctx, &mut manager, "Delete", false);
    assert_eq!(
        manager.take_events(),
        vec![TagManagerEvent::Delete { id: 1 }]
    );
    assert!(!has(&texts, QUESTION), "the question stayed");
    assert!(has(&texts, "x") && has(&texts, "Rename"), "{texts:?}");
    frame(&ctx, &mut manager, vec![]);
    assert_eq!(manager.take_events(), vec![]);
}

#[test]
fn cancel_queues_nothing_and_ends_the_question() {
    let (ctx, mut manager) = asking();
    let texts = press(&ctx, &mut manager, "Cancel", false);
    assert_eq!(manager.take_events(), vec![]);
    assert!(!has(&texts, QUESTION), "the question stayed");
    assert!(manager.is_open());
}

#[test]
fn first_escape_ends_the_question_and_the_second_closes_the_window() {
    let (ctx, mut manager) = asking();
    let texts = frame(&ctx, &mut manager, vec![escape()]);
    assert!(manager.is_open());
    assert_eq!(manager.take_events(), vec![]);
    assert!(!has(&texts, QUESTION), "the question stayed");
    assert!(has(&texts, "Rename"), "{texts:?}");

    frame(&ctx, &mut manager, vec![escape()]);
    assert!(!manager.is_open());
}

#[test]
fn the_question_reads_for_one_image_and_for_none() {
    let (ctx, mut manager) = open_with(vec![row(1, "x", 1)]);
    let texts = press(&ctx, &mut manager, "Delete", false);
    assert!(has(&texts, "Delete 'x' from 1 image?"), "{texts:?}");

    let (ctx, mut manager) = open_with(vec![row(1, "x", 0)]);
    let texts = press(&ctx, &mut manager, "Delete", false);
    assert!(has(&texts, "Delete 'x'?"), "{texts:?}");
}

#[test]
fn starting_a_rename_on_another_row_ends_the_question() {
    let (ctx, mut manager) = open_with(vec![row(1, "x", 12), row(2, "y", 3)]);
    press(&ctx, &mut manager, "Delete", false);
    // Row 1 shows the question, so the only Rename button is on row 2.
    let texts = press(&ctx, &mut manager, "Rename", false);
    assert!(!has(&texts, QUESTION), "the question stayed");
    assert_eq!(count(&texts, "Delete"), 1, "{texts:?}");
    assert_eq!(count(&texts, "Rename"), 1, "{texts:?}");
}

#[test]
fn starting_a_question_ends_a_rename() {
    let (ctx, mut manager) = editing(vec![row(1, "x", 12), row(2, "y", 3)]);
    // Row 1 is in edit, so the only Delete button is on row 2.
    let texts = press(&ctx, &mut manager, "Delete", false);
    assert!(has(&texts, "Delete 'y' from 3 images?"), "{texts:?}");
    // The Delete button of row 1 is back, so its edit ended.
    assert_eq!(count(&texts, "Delete"), 2, "{texts:?}");
    assert_eq!(count(&texts, "Rename"), 1, "{texts:?}");
}

#[test]
fn a_question_on_another_row_moves_the_question() {
    let (ctx, mut manager) = open_with(vec![row(1, "x", 12), row(2, "y", 3)]);
    press(&ctx, &mut manager, "Delete", false);
    let texts = press(&ctx, &mut manager, "Delete", true);
    assert!(!has(&texts, QUESTION), "the first question stayed");
    assert!(has(&texts, "Delete 'y' from 3 images?"), "{texts:?}");
}

#[test]
fn the_question_ends_when_set_rows_drops_its_row() {
    let (ctx, mut manager) = open_with(vec![row(1, "x", 12), row(2, "y", 3)]);
    press(&ctx, &mut manager, "Delete", false);
    manager.set_rows(vec![row(2, "y", 3)]);
    frame(&ctx, &mut manager, vec![]);
    manager.set_rows(vec![row(1, "x", 12), row(2, "y", 3)]);
    let texts = settled(&ctx, &mut manager);
    assert!(!has(&texts, QUESTION), "the old question came back");
}

#[test]
fn the_question_stays_when_set_rows_keeps_its_row() {
    let (ctx, mut manager) = asking();
    manager.set_rows(vec![row(1, "x", 12), row(2, "y", 3)]);
    let texts = settled(&ctx, &mut manager);
    assert!(has(&texts, QUESTION), "{texts:?}");
}

#[test]
fn close_ends_the_question() {
    let (ctx, mut manager) = asking();
    manager.close();
    manager.open();
    let texts = settled(&ctx, &mut manager);
    assert!(!has(&texts, QUESTION), "the question survived close");
}

#[test]
fn the_x_button_ends_the_question() {
    let (ctx, mut manager) = asking();
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
    manager.open();
    let texts = settled(&ctx, &mut manager);
    assert!(!has(&texts, QUESTION), "the question survived the X button");
}

#[test]
fn set_error_ends_the_question() {
    let (ctx, mut manager) = open_with(vec![row(1, "x", 12), row(2, "y", 3)]);
    press(&ctx, &mut manager, "Delete", false);
    manager.set_error(2, "refused".to_owned());
    frame(&ctx, &mut manager, vec![]);
    let texts = frame(&ctx, &mut manager, vec![]);
    assert!(has(&texts, "refused"), "{texts:?}");
    assert!(!has(&texts, QUESTION), "the question stayed");
}
