//! Tests for the selected paths and the selection revision.

use super::*;
use crate::grid::CellClick;

/// The `items` list under the date sort: a, b, a 2, a 10 on screen.
fn dated_gallery() -> Gallery {
    let mut gallery = Gallery::new(SortMode::Date);
    gallery.set_folder(Some(PathBuf::from("/images")));
    gallery.set_items(items());
    gallery
}

/// Click the tile at display `position` with `modifiers`.
fn click_tile(gallery: &mut Gallery, position: usize, modifiers: Modifiers) {
    let index = gallery.visible[position.min(gallery.visible.len() - 1)];
    gallery.apply_grid_click(CellClick {
        index,
        position,
        modifiers,
    });
}

fn path(name: &str) -> PathBuf {
    PathBuf::from("/images").join(name)
}

#[test]
fn an_empty_selection_has_no_paths() {
    assert_eq!(dated_gallery().selected_paths().len(), 0);
}

#[test]
fn the_paths_follow_the_display_order_and_not_the_click_order() {
    let mut gallery = dated_gallery();
    // Click the last tile first, then the first one.
    click_tile(&mut gallery, 3, Modifiers::NONE);
    click_tile(&mut gallery, 0, Modifiers::CTRL);
    click_tile(&mut gallery, 2, Modifiers::CTRL);

    assert_eq!(
        gallery.selected_paths(),
        [path("a.png"), path("a 2.png"), path("a 10.png")]
    );

    // The same set under another sort comes out in that sort's order.
    gallery.set_sort_mode(SortMode::Natural);
    assert_eq!(
        gallery.selected_paths(),
        [path("a 2.png"), path("a 10.png"), path("a.png")]
    );
}

#[test]
fn a_shift_range_lists_its_paths_in_display_order() {
    let mut gallery = dated_gallery();
    click_tile(&mut gallery, 3, Modifiers::NONE);
    click_tile(&mut gallery, 1, Modifiers::SHIFT);
    assert_eq!(
        gallery.selected_paths(),
        [path("b.png"), path("a 2.png"), path("a 10.png")]
    );
}

#[test]
fn the_revision_grows_once_per_real_change() {
    let mut gallery = dated_gallery();
    let start = gallery.selection_revision();

    click_tile(&mut gallery, 1, Modifiers::NONE);
    assert_eq!(gallery.selection_revision(), start + 1, "plain click");

    click_tile(&mut gallery, 1, Modifiers::NONE);
    assert_eq!(gallery.selection_revision(), start + 1, "same sole item");

    click_tile(&mut gallery, 2, Modifiers::CTRL);
    assert_eq!(gallery.selection_revision(), start + 2, "ctrl adds");

    click_tile(&mut gallery, 2, Modifiers::CTRL);
    assert_eq!(gallery.selection_revision(), start + 3, "ctrl removes");

    click_tile(&mut gallery, 3, Modifiers::SHIFT);
    assert_eq!(gallery.selection_revision(), start + 4, "shift range");

    click_tile(&mut gallery, 3, Modifiers::SHIFT);
    assert_eq!(gallery.selection_revision(), start + 4, "same range");

    click_tile(&mut gallery, 99, Modifiers::NONE);
    assert_eq!(
        gallery.selection_revision(),
        start + 4,
        "click off the list"
    );

    gallery.set_items(items());
    assert_eq!(
        gallery.selection_revision(),
        start + 5,
        "clear by new items"
    );

    gallery.set_items(items());
    assert_eq!(gallery.selection_revision(), start + 5, "nothing to clear");
}

#[test]
fn a_frame_with_no_input_keeps_the_revision() {
    let ctx = egui::Context::default();
    crate::theme::apply_theme(&ctx);
    let mut gallery = dated_gallery();
    click_tile(&mut gallery, 0, Modifiers::NONE);
    let revision = gallery.selection_revision();

    for _ in 0..2 {
        frame(&ctx, &mut gallery, Vec::new()).drop_without_applying_deltas();
    }

    assert_eq!(gallery.selection_revision(), revision);
    assert_eq!(gallery.selected_paths(), [path("a.png")]);
}

#[test]
fn a_sort_change_that_reorders_the_selected_paths_grows_the_revision() {
    let mut gallery = dated_gallery();
    click_tile(&mut gallery, 1, Modifiers::NONE);
    click_tile(&mut gallery, 2, Modifiers::CTRL);
    let revision = gallery.selection_revision();
    assert_eq!(gallery.selected_paths(), [path("b.png"), path("a 2.png")]);

    gallery.set_sort_mode(SortMode::Natural);
    assert_eq!(gallery.selected_paths(), [path("a 2.png"), path("b.png")]);
    assert_eq!(gallery.selection_revision(), revision + 1);

    gallery.set_sort_mode(SortMode::Natural);
    assert_eq!(
        gallery.selection_revision(),
        revision + 1,
        "same sort again"
    );
}

#[test]
fn a_sort_change_keeps_the_revision_for_none_or_one_selected_item() {
    let mut gallery = dated_gallery();
    let revision = gallery.selection_revision();
    gallery.set_sort_mode(SortMode::Natural);
    assert_eq!(gallery.selection_revision(), revision, "nothing selected");

    click_tile(&mut gallery, 0, Modifiers::NONE);
    let revision = gallery.selection_revision();
    gallery.set_sort_mode(SortMode::Date);
    assert_eq!(gallery.selection_revision(), revision, "one selected");
}

#[test]
fn a_filter_that_hides_selected_items_keeps_the_rest_and_grows_the_revision() {
    let mut gallery = dated_gallery();
    click_tile(&mut gallery, 0, Modifiers::NONE);
    click_tile(&mut gallery, 2, Modifiers::CTRL);
    let revision = gallery.selection_revision();

    search(&mut gallery, "a 2");

    assert_eq!(gallery.selected_paths(), [path("a 2.png")]);
    assert_eq!(gallery.selection_revision(), revision + 1);
}

#[test]
fn a_filter_that_hides_no_selected_item_keeps_the_revision() {
    let mut gallery = dated_gallery();
    click_tile(&mut gallery, 2, Modifiers::NONE);
    let revision = gallery.selection_revision();

    search(&mut gallery, "a 2");

    assert_eq!(gallery.selected_paths(), [path("a 2.png")]);
    assert_eq!(gallery.selection_revision(), revision);
}

#[test]
fn a_folder_change_that_hides_every_selected_item_empties_the_selection() {
    let mut gallery = Gallery::new(SortMode::Natural);
    gallery.set_items(items_in_two_folders());
    gallery.set_folder(Some(sketches()));
    click_tile(&mut gallery, 0, Modifiers::NONE);
    click_tile(&mut gallery, 1, Modifiers::CTRL);
    assert_eq!(gallery.selected_paths().len(), 2);
    let revision = gallery.selection_revision();

    gallery.set_folder(Some(PathBuf::from("/library/renders")));

    assert_eq!(gallery.selected_paths().len(), 0);
    assert_eq!(gallery.selection_revision(), revision + 1);
}

#[test]
fn a_scope_switch_that_keeps_every_selected_item_keeps_the_revision() {
    let mut gallery = Gallery::new(SortMode::Natural);
    gallery.set_items(items_in_two_folders());
    gallery.set_folder(Some(sketches()));
    click_tile(&mut gallery, 0, Modifiers::NONE);
    let revision = gallery.selection_revision();

    gallery.set_scope(Scope::All);

    assert_eq!(gallery.selection_revision(), revision);
    assert_eq!(gallery.selected_paths().len(), 1);
}

#[test]
fn a_sort_pill_pick_that_reorders_the_selected_paths_grows_the_revision() {
    let mut gallery = dated_gallery();
    click_tile(&mut gallery, 1, Modifiers::NONE);
    click_tile(&mut gallery, 2, Modifiers::CTRL);
    let revision = gallery.selection_revision();

    gallery.pick_sort_mode(SortMode::Natural);

    assert_eq!(gallery.selection_revision(), revision + 1);
    gallery.pick_sort_mode(SortMode::Natural);
    assert_eq!(gallery.selection_revision(), revision + 1, "same pick");
}
