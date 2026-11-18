//! Tests for the Rename dialog.

use super::context_menu::{key, menu_shows, press_at, right_click};
use super::*;
use crate::gallery::rename_dialog::FIELD_ID;
use crate::test_support;
use egui::epaint::Shape;
use egui::text_edit::TextEditState;
use egui::{Context, Key};
use tarragon_core::file_name::FileNameError;

/// Every painted text of a frame, with the rect it covers.
fn painted(output: egui::FullOutput) -> Vec<(String, Rect)> {
    fn collect(shape: &Shape, found: &mut Vec<(String, Rect)>) {
        match shape {
            Shape::Vec(shapes) => shapes.iter().for_each(|s| collect(s, found)),
            Shape::Text(text) => {
                let rect = text.galley.rect.translate(text.pos.to_vec2());
                found.push((text.galley.job.text.clone(), rect));
            }
            _ => {}
        }
    }
    let mut found = Vec::new();
    for clipped in &output.shapes {
        collect(&clipped.shape, &mut found);
    }
    output.drop_without_applying_deltas();
    found
}

/// Draw one frame and return its texts.
pub(super) fn texts(
    ctx: &Context,
    gallery: &mut Gallery,
    events: Vec<Event>,
) -> Vec<(String, Rect)> {
    painted(frame(ctx, gallery, events))
}

/// A gallery over `/images` with the dialog open on the file `name`, settled.
fn open(name: &str) -> (Context, Gallery) {
    open_with(items(), name)
}

/// Like [`open`], over the given items.
fn open_with(items: Vec<GridItem>, name: &str) -> (Context, Gallery) {
    let ctx = test_support::context();
    crate::theme::apply_theme(&ctx);
    let mut gallery = Gallery::new(SortMode::Natural);
    gallery.set_folder(Some(PathBuf::from("/images")));
    gallery.set_items(items);
    gallery.open_rename(PathBuf::from("/images").join(name));
    for _ in 0..3 {
        texts(&ctx, &mut gallery, Vec::new());
    }
    (ctx, gallery)
}

pub(super) fn has_text(seen: &[(String, Rect)], text: &str) -> bool {
    seen.iter().any(|(shown, _)| shown == text)
}

/// Press Enter and let go.
fn enter(ctx: &Context, gallery: &mut Gallery) -> Vec<(String, Rect)> {
    let seen = texts(ctx, gallery, vec![key(Key::Enter)]);
    texts(ctx, gallery, Vec::new());
    seen
}

/// Click the OK button.
fn click_ok(ctx: &Context, gallery: &mut Gallery) {
    click_button(ctx, gallery, "OK");
}

/// Click the button that reads `label`.
pub(super) fn click_button(ctx: &Context, gallery: &mut Gallery, label: &str) {
    let seen = texts(ctx, gallery, Vec::new());
    let (_, rect) = seen
        .iter()
        .find(|(text, _)| text == label)
        .unwrap_or_else(|| panic!("no button reads {label:?}"));
    texts(ctx, gallery, test_support::click(rect.center()));
    texts(ctx, gallery, Vec::new());
}

/// Replace the selected stem with `text`.
fn type_name(ctx: &Context, gallery: &mut Gallery, text: &str) {
    texts(ctx, gallery, vec![Event::Text(text.to_owned())]);
}

#[test]
fn opening_selects_the_stem_and_locks_the_extension() {
    let (ctx, mut gallery) = open("a.png");
    let dialog = gallery.rename.as_ref().expect("the dialog is open");
    assert_eq!(dialog.text(), "a", "the field holds the stem only");
    let state = TextEditState::load(&ctx, Id::new(FIELD_ID)).expect("the field has a state");
    let range = state.cursor.char_range().expect("the stem is selected");
    let [start, end] = range.sorted_cursors();
    assert_eq!(start.index, egui::text::CCursor::new(0).index);
    assert_eq!(end.index, egui::text::CCursor::new(1).index);
    let seen = texts(&ctx, &mut gallery, Vec::new());
    assert!(has_text(&seen, "Rename"), "the title");
    assert!(has_text(&seen, ".png"), "the extension shows as text");

    type_name(&ctx, &mut gallery, "xyz");
    let dialog = gallery.rename.as_ref().expect("the dialog is open");
    assert_eq!(dialog.text(), "xyz", "typing replaced the selected stem");
    let seen = texts(&ctx, &mut gallery, Vec::new());
    assert!(has_text(&seen, ".png"), "the extension is not in the field");
}

#[test]
fn a_bad_name_shows_its_error_and_disables_ok() {
    let cases = [
        ("a/b", FileNameError::ForbiddenChar),
        ("", FileNameError::Empty),
        ("b", FileNameError::Taken),
    ];
    for (name, error) in cases {
        let (ctx, mut gallery) = open("a.png");
        let events = if name.is_empty() {
            vec![key(Key::Backspace)]
        } else {
            vec![Event::Text(name.to_owned())]
        };
        texts(&ctx, &mut gallery, events);
        let seen = texts(&ctx, &mut gallery, Vec::new());
        assert!(has_text(&seen, &error.to_string()), "no error for {name:?}");

        enter(&ctx, &mut gallery);
        click_ok(&ctx, &mut gallery);
        assert!(gallery.rename_open(), "OK was on for {name:?}");
        assert_eq!(gallery.take_menu_events(), vec![], "event for {name:?}");
    }
}

#[test]
fn enter_on_a_new_name_raises_rename_to_with_the_stem_only() {
    let (ctx, mut gallery) = open("a.png");
    type_name(&ctx, &mut gallery, "new");
    enter(&ctx, &mut gallery);
    assert_eq!(
        gallery.take_menu_events(),
        vec![GridMenuEvent::RenameTo {
            path: PathBuf::from("/images/a.png"),
            new_stem: "new".to_owned(),
        }]
    );
    assert!(!gallery.rename_open(), "the dialog closed");
}

#[test]
fn ok_on_a_new_name_raises_rename_to() {
    let (ctx, mut gallery) = open("a.png");
    type_name(&ctx, &mut gallery, "new");
    click_ok(&ctx, &mut gallery);
    assert_eq!(gallery.take_menu_events().len(), 1);
    assert!(!gallery.rename_open(), "the dialog closed");
}

#[test]
fn the_same_name_closes_with_no_event_even_when_it_fails_the_rules() {
    for name in ["a.png", ".hidden", "a .png"] {
        let (ctx, mut gallery) = open(name);
        let seen = texts(&ctx, &mut gallery, Vec::new());
        let error = [FileNameError::EdgeSpaceOrDot, FileNameError::Empty]
            .iter()
            .any(|error| has_text(&seen, &error.to_string()));
        assert!(!error, "an error shows for the current name {name:?}");

        enter(&ctx, &mut gallery);
        assert_eq!(gallery.take_menu_events(), vec![], "event for {name:?}");
        assert!(!gallery.rename_open(), "the dialog stayed for {name:?}");
    }
}

/// An item of `/images` or another folder, by its whole path.
fn item_at(path: &str) -> GridItem {
    GridItem::new(PathBuf::from(path), 1, None)
}

#[test]
fn a_name_that_ends_in_a_dot_is_checked_with_the_dot() {
    let (ctx, mut gallery) = open_with(vec![item_at("/images/a."), item_at("/images/b")], "a.");
    let seen = texts(&ctx, &mut gallery, Vec::new());
    assert!(has_text(&seen, "."), "the locked extension shows the dot");
    type_name(&ctx, &mut gallery, "b");
    enter(&ctx, &mut gallery);
    assert_eq!(gallery.take_menu_events().len(), 1, "b. is free");

    let items = vec![
        item_at("/images/a."),
        item_at("/images/b"),
        item_at("/images/b."),
    ];
    let (ctx, mut gallery) = open_with(items, "a.");
    type_name(&ctx, &mut gallery, "b");
    let seen = texts(&ctx, &mut gallery, Vec::new());
    assert!(has_text(&seen, &FileNameError::Taken.to_string()));
    enter(&ctx, &mut gallery);
    assert_eq!(gallery.take_menu_events(), vec![]);
    assert!(gallery.rename_open(), "the dialog stayed");
}

#[test]
fn typing_in_two_frames_keeps_both_letters() {
    let (ctx, mut gallery) = open("a.png");
    type_name(&ctx, &mut gallery, "x");
    type_name(&ctx, &mut gallery, "y");
    let dialog = gallery.rename.as_ref().expect("the dialog is open");
    assert_eq!(dialog.text(), "xy", "the selection came back");
}

#[test]
fn a_file_in_another_folder_does_not_count_as_taken() {
    let items = vec![item_at("/images/a.png"), item_at("/other/z.png")];
    let (ctx, mut gallery) = open_with(items, "a.png");
    type_name(&ctx, &mut gallery, "z");
    enter(&ctx, &mut gallery);
    assert_eq!(gallery.take_menu_events().len(), 1);
}

#[test]
fn cancel_closes_with_no_event() {
    let (ctx, mut gallery) = open("a.png");
    type_name(&ctx, &mut gallery, "new");
    click_button(&ctx, &mut gallery, "Cancel");
    assert!(!gallery.rename_open(), "the dialog closed");
    assert_eq!(gallery.take_menu_events(), vec![]);
}

#[test]
fn escape_closes_with_no_event_and_is_consumed() {
    let (ctx, mut gallery) = open("a.png");
    type_name(&ctx, &mut gallery, "new");
    let input = test_support::input(900.0, vec![key(Key::Escape)]);
    let mut seen_later = true;
    ctx.run_ui(input, |ui| {
        gallery.show(ui);
        seen_later = ui.input(|input| input.key_pressed(Key::Escape));
    })
    .drop_without_applying_deltas();
    assert!(!seen_later, "Escape was left for another handler");
    assert!(!gallery.rename_open(), "the dialog closed");
    assert_eq!(gallery.take_menu_events(), vec![]);
}

#[test]
fn opening_the_dialog_closes_an_open_tile_menu() {
    let (ctx, mut gallery) = super::context_menu::setup();
    let (_, seen) = right_click(&ctx, &mut gallery, 1);
    assert!(menu_shows(&seen), "the menu opened");

    gallery.open_rename(PathBuf::from("/images/b.png"));
    for _ in 0..2 {
        super::context_menu::run(&ctx, &mut gallery, Vec::new());
    }
    gallery.rename = None;
    let seen = super::context_menu::run(&ctx, &mut gallery, Vec::new());
    assert!(!menu_shows(&seen), "the menu stayed open");
}

#[test]
fn a_right_click_does_not_open_a_menu_while_the_dialog_is_open() {
    let (ctx, mut gallery) = super::context_menu::setup();
    gallery.open_rename(PathBuf::from("/images/b.png"));
    for _ in 0..2 {
        super::context_menu::run(&ctx, &mut gallery, Vec::new());
    }
    // Tile 0 lies outside the dialog window, so only the dim layer can keep
    // the click from it.
    let pos = super::context_menu::tile_center(&ctx, 0);
    let window = ctx
        // egui keys a window by the optional text of its title.
        .memory(|memory| memory.area_rect(Id::new(Some("Rename"))))
        .expect("the dialog window was drawn");
    assert!(!window.contains(pos), "the tile is under the window");
    let seen = press_at(&ctx, &mut gallery, pos, PointerButton::Secondary);
    assert!(!menu_shows(&seen), "a menu opened over the dialog");
    gallery.rename = None;
    let seen = super::context_menu::run(&ctx, &mut gallery, Vec::new());
    assert!(!menu_shows(&seen), "a menu waited under the dialog");
}

#[test]
fn a_rename_does_not_open_while_a_delete_dialog_is_open() {
    let (ctx, mut gallery) = super::context_menu::setup();
    gallery.open_delete(vec![PathBuf::from("/images/a.png")], false);
    gallery.open_rename(PathBuf::from("/images/b.png"));
    assert!(!gallery.rename_open(), "a Rename dialog opened over Delete");
    for _ in 0..2 {
        super::context_menu::run(&ctx, &mut gallery, Vec::new());
    }
    assert!(gallery.delete_open(), "the Delete dialog stayed");
}
