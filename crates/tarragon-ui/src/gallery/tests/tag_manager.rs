//! Tests for the "Manage tags" window inside the gallery.

use super::filter_bar::{click_button, listed, texts};
use super::*;
use crate::test_support;

fn row(id: i64, name: &str, count: u32) -> TagRow {
    TagRow {
        id,
        name: name.to_owned(),
        count,
    }
}

/// Every text drawn in one frame.
fn shown(ctx: &egui::Context, gallery: &mut Gallery, events: Vec<Event>) -> Vec<(String, Pos2)> {
    let out = frame(ctx, gallery, events);
    let found = texts(&out);
    out.drop_without_applying_deltas();
    found
}

fn has(found: &[(String, Pos2)], want: &str) -> bool {
    found.iter().any(|t| t.0 == want)
}

fn has_text(found: &[String], want: &str) -> bool {
    found.iter().any(|t| t == want)
}

/// Click the text `label`, then draw one more frame and return its texts.
fn press(ctx: &egui::Context, gallery: &mut Gallery, label: &str) -> Vec<(String, Pos2)> {
    let at = shown(ctx, gallery, Vec::new())
        .into_iter()
        .find(|t| t.0 == label)
        .unwrap_or_else(|| panic!("a {label} button"))
        .1;
    shown(ctx, gallery, test_support::click(at + vec2(4.0, 4.0)));
    shown(ctx, gallery, Vec::new())
}

/// A gallery with the window open and one tag row, drawn until it settles.
fn open_with_row() -> (egui::Context, Gallery) {
    let ctx = test_support::context();
    crate::theme::apply_theme(&ctx);
    let mut gallery = Gallery::new(SortMode::Natural);
    gallery.set_tag_rows(vec![row(1, "x", 12)]);
    gallery.tag_manager.open();
    shown(&ctx, &mut gallery, Vec::new());
    shown(&ctx, &mut gallery, Vec::new());
    (ctx, gallery)
}

fn key(key: egui::Key, modifiers: Modifiers) -> Event {
    Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers,
    }
}

#[test]
fn the_footer_row_opens_the_window_in_the_click_frame() {
    let (ctx, mut gallery) = listed();
    click_button(&ctx, &mut gallery, "Filter Tags");
    let at = shown(&ctx, &mut gallery, Vec::new())
        .into_iter()
        .find(|t| t.0 == "Manage tags...")
        .expect("the footer row")
        .1;
    assert!(!gallery.tag_manager.is_open());
    shown(&ctx, &mut gallery, test_support::click(at + vec2(4.0, 4.0)));
    assert!(gallery.tag_manager.is_open());
    shown(&ctx, &mut gallery, Vec::new());
    assert!(has(&shown(&ctx, &mut gallery, Vec::new()), "Manage tags"));
}

#[test]
fn one_footer_click_opens_once_and_asks_for_the_rows() {
    let (ctx, mut gallery) = listed();
    click_button(&ctx, &mut gallery, "Filter Tags");
    gallery.take_filter_menu_opened();
    let at = shown(&ctx, &mut gallery, Vec::new())
        .into_iter()
        .find(|t| t.0 == "Manage tags...")
        .expect("the footer row")
        .1;
    shown(&ctx, &mut gallery, test_support::click(at + vec2(4.0, 4.0)));
    assert!(gallery.take_filter_menu_opened());
    assert!(!gallery.take_filter_menu_opened());
    // The user closes the window. Later frames must not open it again.
    shown(&ctx, &mut gallery, Vec::new());
    shown(
        &ctx,
        &mut gallery,
        vec![key(egui::Key::Escape, Modifiers::NONE)],
    );
    assert!(!gallery.tag_manager.is_open());
    for _ in 0..3 {
        shown(&ctx, &mut gallery, Vec::new());
    }
    assert!(!gallery.tag_manager.is_open());
    assert!(!gallery.take_filter_menu_opened());
}

#[test]
fn one_escape_closes_the_window_and_the_open_menu() {
    let (ctx, mut gallery) = listed();
    let open = click_button(&ctx, &mut gallery, "Filter Tags");
    assert!(has_text(&open, "lighting"));
    gallery.tag_manager.open();
    shown(&ctx, &mut gallery, Vec::new());
    shown(&ctx, &mut gallery, Vec::new());
    shown(
        &ctx,
        &mut gallery,
        vec![key(egui::Key::Escape, Modifiers::NONE)],
    );
    let found = shown(&ctx, &mut gallery, Vec::new());
    assert!(!gallery.tag_manager.is_open());
    assert!(!has(&found, "lighting"));
}

#[test]
fn a_closed_manager_draws_no_window() {
    let ctx = test_support::context();
    let mut gallery = Gallery::new(SortMode::Natural);
    gallery.set_tag_rows(vec![row(1, "x", 12)]);
    shown(&ctx, &mut gallery, Vec::new());
    let found = shown(&ctx, &mut gallery, Vec::new());
    assert!(!has(&found, "Manage tags") && !has(&found, "x"));
}

#[test]
fn rows_show_and_a_rename_comes_out_once() {
    let (ctx, mut gallery) = open_with_row();
    let found = shown(&ctx, &mut gallery, Vec::new());
    assert!(has(&found, "x") && has(&found, "12 images"));
    press(&ctx, &mut gallery, "Rename");
    shown(&ctx, &mut gallery, Vec::new());
    let replace = vec![
        key(egui::Key::A, Modifiers::COMMAND),
        Event::Text("y".to_owned()),
    ];
    shown(&ctx, &mut gallery, replace);
    shown(
        &ctx,
        &mut gallery,
        vec![key(egui::Key::Enter, Modifiers::NONE)],
    );
    shown(&ctx, &mut gallery, Vec::new());
    let rename = TagManagerEvent::Rename {
        id: 1,
        new_name: "y".to_owned(),
    };
    assert_eq!(gallery.take_tag_manager_events(), vec![rename]);
    assert_eq!(gallery.take_tag_manager_events(), vec![]);
}

#[test]
fn a_delete_confirm_comes_out_as_delete() {
    let (ctx, mut gallery) = open_with_row();
    let found = press(&ctx, &mut gallery, "Delete");
    assert!(has(&found, "Delete 'x' from 12 images?"));
    assert_eq!(gallery.take_tag_manager_events(), vec![]);
    press(&ctx, &mut gallery, "Delete");
    assert_eq!(
        gallery.take_tag_manager_events(),
        vec![TagManagerEvent::Delete { id: 1 }]
    );
}

#[test]
fn a_rename_error_reaches_the_row() {
    let (ctx, mut gallery) = open_with_row();
    gallery.set_tag_manager_error(1, "refused");
    shown(&ctx, &mut gallery, Vec::new());
    let found = shown(&ctx, &mut gallery, Vec::new());
    // A rename error puts the row in edit, so the Rename button is gone.
    assert!(has(&found, "refused") && !has(&found, "Rename"));
}

#[test]
fn a_delete_error_reaches_the_row() {
    let (ctx, mut gallery) = open_with_row();
    gallery.set_tag_manager_delete_error(1, String::from("in use"));
    shown(&ctx, &mut gallery, Vec::new());
    let found = shown(&ctx, &mut gallery, Vec::new());
    // The message takes the count's place and the row keeps its buttons.
    assert!(has(&found, "in use") && has(&found, "Rename"));
    assert!(!has(&found, "12 images"));
}
