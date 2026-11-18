//! Tests for the select all and focus search hooks.

use egui::TextEdit;
use egui::text::{CCursor, CCursorRange};

use super::*;

/// The id of the search field.
fn search_id() -> Id {
    Id::new("tarragon-gallery-search")
}

/// A themed context and a gallery that holds [`items`] and has drawn once.
fn drawn_gallery() -> (egui::Context, Gallery) {
    let ctx = egui::Context::default();
    crate::theme::apply_theme(&ctx);
    let mut gallery = Gallery::new(SortMode::Natural);
    gallery.set_folder(Some(PathBuf::from("/images")));
    gallery.set_items(items());
    frame(&ctx, &mut gallery, Vec::new()).drop_without_applying_deltas();
    (ctx, gallery)
}

/// Whether the search field has focus.
fn search_has_focus(ctx: &egui::Context) -> bool {
    ctx.memory(|memory| memory.has_focus(search_id()))
}

#[test]
fn select_all_takes_the_order_and_anchors_on_its_first_entry() {
    let mut selection = Selection::from_parts([9], Some(9));
    selection.select_all(&[3, 1, 2]);
    assert_eq!(selection.iter().collect::<Vec<_>>(), [1, 2, 3]);
    assert_eq!(selection.anchor(), Some(3));
}

#[test]
fn select_all_on_an_empty_order_holds_nothing() {
    let mut selection = Selection::from_parts([4], Some(4));
    selection.select_all(&[]);
    assert!(selection.is_empty());
    assert_eq!(selection.anchor(), None);
}

#[test]
fn select_all_visible_skips_items_the_search_hides() {
    let mut gallery = Gallery::new(SortMode::Natural);
    gallery.set_folder(Some(PathBuf::from("/images")));
    let mut all = items();
    all.push(GridItem::new(PathBuf::from("/images/c.jpg"), 1, None));
    gallery.set_items(all);
    search(&mut gallery, "png");
    assert_eq!(gallery.visible_len(), 4);
    let before = gallery.selection_revision();

    gallery.select_all_visible();

    assert_eq!(gallery.selection.len(), 4, "only the visible items");
    assert_eq!(gallery.selected_paths().len(), 4);
    assert_eq!(gallery.selection_revision(), before + 1);
}

#[test]
fn select_all_visible_twice_grows_the_revision_once() {
    let mut gallery = Gallery::new(SortMode::Natural);
    gallery.set_folder(Some(PathBuf::from("/images")));
    gallery.set_items(items());
    let before = gallery.selection_revision();

    gallery.select_all_visible();
    gallery.select_all_visible();

    assert_eq!(gallery.selection_revision(), before + 1);
}

#[test]
fn focus_search_focuses_the_field_once_and_a_click_elsewhere_removes_it() {
    let (ctx, mut gallery) = drawn_gallery();
    assert!(!search_has_focus(&ctx));

    gallery.focus_search();
    frame(&ctx, &mut gallery, Vec::new()).drop_without_applying_deltas();
    assert!(search_has_focus(&ctx), "the request focused the field");

    frame(&ctx, &mut gallery, Vec::new()).drop_without_applying_deltas();
    assert!(search_has_focus(&ctx), "a quiet frame keeps the focus");

    click(
        &ctx,
        &mut gallery,
        Id::new(("tarragon-scope-tab", "All Images")),
    );
    assert!(
        !search_has_focus(&ctx),
        "the flag did not refocus the field"
    );
}

#[test]
fn focus_search_selects_the_text_so_typing_replaces_it() {
    let (ctx, mut gallery) = drawn_gallery();
    gallery.search = "abc".to_owned();
    gallery.focus_search();
    frame(&ctx, &mut gallery, Vec::new()).drop_without_applying_deltas();

    let state = TextEdit::load_state(&ctx, search_id()).expect("the field stored its state");
    assert_eq!(
        state.cursor.char_range(),
        Some(CCursorRange::two(CCursor::new(0), CCursor::new(3)))
    );

    frame(&ctx, &mut gallery, vec![Event::Text("x".to_owned())]).drop_without_applying_deltas();
    assert_eq!(gallery.search, "x", "the typed text replaced the old text");
}

#[test]
fn focus_search_counts_characters_not_bytes() {
    let (ctx, mut gallery) = drawn_gallery();
    gallery.search = "äö".to_owned();
    gallery.focus_search();
    frame(&ctx, &mut gallery, Vec::new()).drop_without_applying_deltas();

    let state = TextEdit::load_state(&ctx, search_id()).expect("the field stored its state");
    assert_eq!(
        state.cursor.char_range(),
        Some(CCursorRange::two(CCursor::new(0), CCursor::new(2)))
    );
}
