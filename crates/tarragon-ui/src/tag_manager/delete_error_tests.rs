//! Tests for the failed-delete message and the button layout of the rows.

use super::TagManager;
use super::WIDTH;
use super::delete_tests::{QUESTION, asking, has, open_with, press};
use super::rename::MAX_ERROR_SHARE;
use super::tests::{count, editing, escape, frame, frame_at, frame_rects, row, settled};
use egui::{Context, Order, Pos2, Rect};

const MESSAGE: &str = "could not delete";

/// Two rows; a failed delete is shown on row 1.
fn failed() -> (Context, TagManager) {
    let (ctx, mut manager) = open_with(vec![row(1, "x", 12), row(2, "y", 3)]);
    manager.set_delete_error(1, MESSAGE);
    settled(&ctx, &mut manager);
    (ctx, manager)
}

#[test]
fn the_message_shows_on_its_row_and_a_missing_row_is_ignored() {
    let (ctx, mut manager) = failed();
    let texts = frame(&ctx, &mut manager, vec![]);
    assert!(has(&texts, MESSAGE), "{texts:?}");
    assert!(!has(&texts, "12 images"), "the count stayed: {texts:?}");
    assert!(has(&texts, "3 images"), "{texts:?}");

    let (ctx, mut manager) = open_with(vec![row(1, "x", 12)]);
    manager.set_delete_error(9, MESSAGE);
    let texts = settled(&ctx, &mut manager);
    assert!(!has(&texts, MESSAGE), "{texts:?}");
}

#[test]
fn the_message_ends_the_question() {
    let (ctx, mut manager) = asking();
    manager.set_delete_error(1, MESSAGE);
    settled(&ctx, &mut manager);
    let texts = frame(&ctx, &mut manager, vec![]);
    assert!(has(&texts, MESSAGE), "{texts:?}");
    assert!(!has(&texts, QUESTION), "the question stayed");
}

#[test]
fn the_message_ends_a_rename() {
    let (ctx, mut manager) = editing(vec![row(1, "x", 12), row(2, "y", 3)]);
    manager.set_delete_error(1, MESSAGE);
    settled(&ctx, &mut manager);
    let texts = frame(&ctx, &mut manager, vec![]);
    assert!(has(&texts, MESSAGE), "{texts:?}");
    // Both rows are plain again, so both have a Rename button.
    assert_eq!(count(&texts, "Rename"), 2, "{texts:?}");
}

#[test]
fn a_rename_click_clears_the_message() {
    let (ctx, mut manager) = failed();
    let texts = press(&ctx, &mut manager, "Rename", true);
    assert!(!has(&texts, MESSAGE), "{texts:?}");
    assert!(has(&texts, "12 images"), "{texts:?}");
}

#[test]
fn a_delete_click_clears_the_message() {
    let (ctx, mut manager) = failed();
    let texts = press(&ctx, &mut manager, "Delete", true);
    assert!(!has(&texts, MESSAGE), "{texts:?}");
    assert!(has(&texts, "Delete 'y' from 3 images?"), "{texts:?}");
    assert!(has(&texts, "12 images"), "{texts:?}");
}

#[test]
fn set_rows_drops_the_message_with_its_row_and_keeps_it_otherwise() {
    let (ctx, mut manager) = failed();
    manager.set_rows(vec![row(1, "x", 12), row(2, "y", 4)]);
    let texts = settled(&ctx, &mut manager);
    assert!(has(&texts, MESSAGE), "{texts:?}");

    manager.set_rows(vec![row(2, "y", 4)]);
    frame(&ctx, &mut manager, vec![]);
    manager.set_rows(vec![row(1, "x", 12), row(2, "y", 4)]);
    let texts = settled(&ctx, &mut manager);
    assert!(!has(&texts, MESSAGE), "the old message came back");
}

#[test]
fn close_clears_the_message() {
    let (ctx, mut manager) = failed();
    manager.close();
    manager.open();
    let texts = settled(&ctx, &mut manager);
    assert!(!has(&texts, MESSAGE), "the message survived close");
}

/// The x of the first text painted as `label`.
fn x_of(shown: &[(String, Pos2)], label: &str) -> f32 {
    shown
        .iter()
        .find(|(text, _)| text == label)
        .unwrap_or_else(|| panic!("a {label} button"))
        .1
        .x
}

#[test]
fn rename_is_left_of_delete_and_delete_is_left_of_cancel() {
    let (ctx, mut manager) = open_with(vec![row(1, "x", 12)]);
    let shown = frame_at(&ctx, &mut manager, vec![]);
    assert!(x_of(&shown, "Rename") < x_of(&shown, "Delete"));

    press(&ctx, &mut manager, "Delete", false);
    let shown = frame_at(&ctx, &mut manager, vec![]);
    assert!(x_of(&shown, "Delete") < x_of(&shown, "Cancel"));
}

fn window_right(ctx: &Context) -> f32 {
    ctx.memory(|m| {
        let layers = m.areas().visible_layer_ids();
        let window = layers.into_iter().find(|l| l.order == Order::Middle)?;
        m.area_rect(window.id)
    })
    .expect("the window area")
    .right()
}

type Shown = Vec<(String, Pos2)>;

/// The x of Rename and Delete in a plain row, everything painted, and the
/// window's right edge. `message` is a failed delete shown on the row.
fn plain_row_at(name: &str, message: Option<&str>) -> (f32, f32, Shown, f32) {
    let (ctx, mut manager) = open_with(vec![row(1, name, 12)]);
    if let Some(text) = message {
        manager.set_delete_error(1, text);
        settled(&ctx, &mut manager);
    }
    let shown = frame_at(&ctx, &mut manager, vec![]);
    let at = (x_of(&shown, "Rename"), x_of(&shown, "Delete"));
    (at.0, at.1, shown, window_right(&ctx))
}

/// The rect of the failed-delete `message` painted on the row `name`.
fn message_rect(name: &str, message: &str) -> Rect {
    let (ctx, mut manager) = open_with(vec![row(1, name, 12)]);
    manager.set_delete_error(1, message);
    settled(&ctx, &mut manager);
    frame_rects(&ctx, &mut manager, vec![])
        .into_iter()
        .find(|(text, _, _)| text == message)
        .expect("the message painted")
        .2
}

/// The x of Delete and Cancel in the question row.
fn question_row_at(name: &str) -> (f32, f32, f32) {
    let (ctx, mut manager) = open_with(vec![row(1, name, 12)]);
    press(&ctx, &mut manager, "Delete", false);
    let shown = frame_at(&ctx, &mut manager, vec![]);
    let (delete, cancel) = (x_of(&shown, "Delete"), x_of(&shown, "Cancel"));
    (delete, cancel, window_right(&ctx))
}

#[test]
fn a_long_name_keeps_the_buttons_where_a_short_name_puts_them() {
    let (rename, delete, _, right) = plain_row_at("x", None);
    assert!(delete < right);
    let (long_rename, long_delete, _, _) = plain_row_at(&"n".repeat(200), None);
    assert_eq!((long_rename, long_delete), (rename, delete));

    let (delete, cancel, right) = question_row_at("x");
    assert!(cancel < right);
    let (long_delete, long_cancel, _) = question_row_at(&"n".repeat(200));
    assert_eq!((long_delete, long_cancel), (delete, cancel));
}

#[test]
fn a_long_message_leaves_the_name_and_the_buttons() {
    let (rename, delete, _, right) = plain_row_at("sky", None);
    let long = "m".repeat(200);
    let (long_rename, long_delete, _, _) = plain_row_at("sky", Some(&long));
    // The message takes at most its share of the row. A painted text's own
    // rect is used because its position is its right edge in this layout.
    let message = message_rect("sky", &long);
    let most = WIDTH * MAX_ERROR_SHARE + 0.5;
    assert!(
        message.width() <= most,
        "the message is {} wide, more than {most}; it spans x {} to {}",
        message.width(),
        message.min.x,
        message.max.x
    );
    assert!(long_delete < right);
    assert_eq!((long_rename, long_delete), (rename, delete));
}

#[test]
fn a_refused_rename_clears_the_delete_message() {
    let (ctx, mut manager) = failed();
    manager.set_error(1, "refused");
    settled(&ctx, &mut manager);
    let texts = frame(&ctx, &mut manager, vec![escape()]);
    assert!(!has(&texts, MESSAGE), "{texts:?}");
    assert!(has(&texts, "12 images"), "{texts:?}");
}
