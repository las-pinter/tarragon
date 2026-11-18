//! Tests for the Paste menu on the empty-folder card.

use super::context_menu::{Text, assert_no_events, click_row, key, press_at, run, setup};
use super::empty_space_menu::paste_menu_shows;
use super::*;
use egui::Key;
use std::collections::HashSet;

/// A gallery with the folder `/images` open and no image. Returns the texts of
/// the last frame.
fn card_setup() -> (egui::Context, Gallery, Vec<Text>) {
    let (ctx, _) = setup();
    let mut gallery = Gallery::new(SortMode::Natural);
    gallery.set_folder(Some(PathBuf::from("/images")));
    let mut seen = Vec::new();
    for _ in 0..4 {
        seen = run(&ctx, &mut gallery, Vec::new());
    }
    (ctx, gallery, seen)
}

/// The centre of the painted text `label`.
fn text_center(seen: &[Text], label: &str) -> Pos2 {
    let (_, rect, _) = seen
        .iter()
        .find(|(text, ..)| text == label)
        .unwrap_or_else(|| panic!("no text reads {label:?}"));
    rect.center()
}

/// The point left of the card line `label`, on the card area but off its text.
fn beside(seen: &[Text], label: &str) -> Pos2 {
    Pos2::new(100.0, text_center(seen, label).y)
}

/// Right-click beside the card line `label`. Returns the texts of the frame
/// after.
fn right_click_card(
    ctx: &egui::Context,
    gallery: &mut Gallery,
    seen: &[Text],
    label: &str,
) -> Vec<Text> {
    press_at(ctx, gallery, beside(seen, label), PointerButton::Secondary)
}

/// Right-click at `pos`. Returns whether the card menu showed or opened in any
/// frame, so a menu that opens and closes again is caught.
fn right_click_ever_shows(ctx: &egui::Context, gallery: &mut Gallery, pos: Pos2) -> bool {
    let press = |pressed| Event::PointerButton {
        pos,
        button: PointerButton::Secondary,
        pressed,
        modifiers: Modifiers::NONE,
    };
    let mut shown = false;
    for events in [
        vec![Event::PointerMoved(pos)],
        vec![press(true)],
        vec![press(false)],
        Vec::new(),
    ] {
        shown |= paste_menu_shows(&run(ctx, gallery, events));
        // The menu paints a frame after it opens, so also read its state.
        shown |= gallery.tile_menu.from_card;
    }
    shown
}

#[test]
fn a_right_click_on_the_empty_folder_card_opens_the_paste_menu() {
    let (ctx, mut gallery, seen) = card_setup();
    assert_eq!(gallery.empty_kind(), Some(EmptyKind::EmptyFolder));
    let line = "No supported images in this folder";
    let seen = right_click_card(&ctx, &mut gallery, &seen, line);
    assert!(paste_menu_shows(&seen), "the card did not open the menu");
    let after = click_row(&ctx, &mut gallery, &seen, "Paste");
    assert!(!paste_menu_shows(&after), "Paste left the menu open");
    assert_eq!(gallery.take_menu_events(), [GridMenuEvent::Paste]);
    let seen = right_click_card(&ctx, &mut gallery, &after, line);
    assert!(paste_menu_shows(&seen));
    run(&ctx, &mut gallery, vec![key(Key::Escape)]);
    let after = run(&ctx, &mut gallery, Vec::new());
    assert!(!paste_menu_shows(&after), "Escape left the menu open");
    assert_no_events(&mut gallery, "Escape");
}

#[test]
fn a_right_click_on_the_card_text_opens_the_paste_menu() {
    let (ctx, mut gallery, seen) = card_setup();
    let hint = seen
        .iter()
        .map(|(text, ..)| text.as_str())
        .find(|text| text.starts_with("This folder has no file"))
        .expect("the hint is painted");
    for line in ["No supported images in this folder", hint] {
        let at = text_center(&seen, line);
        let after = press_at(&ctx, &mut gallery, at, PointerButton::Secondary);
        assert!(paste_menu_shows(&after), "{line:?} did not open the menu");
        run(&ctx, &mut gallery, vec![key(Key::Escape)]);
        run(&ctx, &mut gallery, Vec::new());
    }
}

#[test]
fn the_no_folder_and_no_result_cards_open_no_menu() {
    let (ctx, _) = setup();
    let mut gallery = Gallery::new(SortMode::Natural);
    let seen = run(&ctx, &mut gallery, Vec::new());
    let at = beside(&seen, "Open a folder to begin");
    assert!(
        !right_click_ever_shows(&ctx, &mut gallery, at),
        "the no-folder card opened a menu"
    );

    let (ctx, mut gallery, _) = card_setup();
    gallery.set_filter_result(Some(HashSet::new()));
    let seen = run(&ctx, &mut gallery, Vec::new());
    assert_eq!(gallery.empty_kind(), Some(EmptyKind::NoResult));
    let at = beside(&seen, "No images match");
    assert!(
        !right_click_ever_shows(&ctx, &mut gallery, at),
        "the no-result card opened a menu"
    );
}

/// A gallery on the empty-folder card with its Paste menu open.
fn card_menu_open() -> (egui::Context, Gallery) {
    let (ctx, mut gallery, seen) = card_setup();
    let line = "No supported images in this folder";
    let seen = right_click_card(&ctx, &mut gallery, &seen, line);
    assert!(paste_menu_shows(&seen), "the card menu did not open");
    (ctx, gallery)
}

#[test]
fn the_card_menu_closes_when_the_folder_closes_to_the_no_folder_card() {
    let (ctx, mut gallery) = card_menu_open();
    gallery.set_folder(None);
    let after = run(&ctx, &mut gallery, Vec::new());
    assert_eq!(gallery.empty_kind(), Some(EmptyKind::NoFolder));
    assert!(
        !paste_menu_shows(&after),
        "the no-folder card kept the menu"
    );
}

#[test]
fn the_card_menu_closes_when_a_filter_makes_the_no_result_card() {
    let (ctx, mut gallery) = card_menu_open();
    gallery.set_filter_result(Some(HashSet::new()));
    let after = run(&ctx, &mut gallery, Vec::new());
    assert_eq!(gallery.empty_kind(), Some(EmptyKind::NoResult));
    assert!(
        !paste_menu_shows(&after),
        "the no-result card kept the menu"
    );
}

#[test]
fn the_card_button_keeps_its_click_and_is_the_only_tab_stop() {
    let (ctx, mut gallery, seen) = card_setup();
    let button = text_center(&seen, "Open Another Folder...");
    press_at(&ctx, &mut gallery, button, PointerButton::Primary);
    assert!(gallery.take_open_folder_request(), "the click did nothing");
    // Press Tab 40 times. No Tab stop may cover a card point off the button
    // (the sensor would), and the button must take focus at least once.
    let off = beside(&seen, "Open Another Folder...");
    let mut reached = false;
    for _ in 0..40 {
        run(&ctx, &mut gallery, vec![key(Key::Tab)]);
        run(&ctx, &mut gallery, Vec::new());
        let Some(rect) = ctx
            .memory(egui::Memory::focused)
            .and_then(|id| ctx.read_response(id))
            .map(|response| response.rect)
        else {
            continue;
        };
        assert!(!rect.contains(off), "a Tab stop covers the card");
        reached |= rect.contains(button);
    }
    assert!(reached, "the button never took focus");
}

#[test]
fn the_card_menu_closes_when_the_folder_gains_files_or_closes() {
    let (ctx, mut gallery, seen) = card_setup();
    let line = "No supported images in this folder";
    let seen = right_click_card(&ctx, &mut gallery, &seen, line);
    assert!(paste_menu_shows(&seen));
    gallery.set_items(items());
    let after = run(&ctx, &mut gallery, Vec::new());
    assert!(!paste_menu_shows(&after), "the grid still shows the menu");
    // The menu stays closed when the card returns.
    gallery.set_items(Vec::new());
    let after = run(&ctx, &mut gallery, Vec::new());
    assert_eq!(gallery.empty_kind(), Some(EmptyKind::EmptyFolder));
    assert!(!paste_menu_shows(&after), "the menu came back");

    let (ctx, mut gallery, seen) = card_setup();
    let seen = right_click_card(&ctx, &mut gallery, &seen, line);
    assert!(paste_menu_shows(&seen));
    gallery.set_folder(None);
    let after = run(&ctx, &mut gallery, Vec::new());
    assert!(!paste_menu_shows(&after), "the closed folder kept the menu");
}
