//! Tests for the sort order and the stale selection.

use super::*;

#[test]
fn set_items_sorts_in_the_current_mode() {
    let mut gallery = Gallery::new(SortMode::Natural);
    gallery.set_folder(Some(PathBuf::from("/images")));
    gallery.set_items(items());
    assert_eq!(gallery.order, [3, 2, 1, 0]);

    let mut dated = Gallery::new(SortMode::Date);
    dated.set_folder(Some(PathBuf::from("/images")));
    dated.set_items(items());
    assert_eq!(dated.order, [1, 0, 3, 2]);
}

#[test]
fn set_items_drops_stale_selection_indices() {
    let mut gallery = Gallery::new(SortMode::Natural);
    gallery.set_items(items());
    gallery.selection.select_only(3);
    let only = GridItem::new(PathBuf::from("/images/a.png"), 1, None);
    gallery.set_items(vec![only]);
    assert!(gallery.selection.is_empty(), "index 3 is now stale");
}

#[test]
fn set_sort_mode_resorts_and_clears_the_changed_flag() {
    let mut gallery = Gallery::new(SortMode::Natural);
    gallery.set_folder(Some(PathBuf::from("/images")));
    gallery.set_items(items());
    gallery.sort_changed = true;

    gallery.set_sort_mode(SortMode::Date);
    assert_eq!(gallery.sort_mode(), SortMode::Date);
    assert_eq!(gallery.order, [1, 0, 3, 2], "the order follows the mode");
    assert_eq!(gallery.take_sort_changed(), None, "the flag cleared");
}

#[test]
fn the_sort_labels_name_the_two_modes() {
    assert_eq!(sort_label(SortMode::Natural), "Name");
    assert_eq!(sort_label(SortMode::Date), "Date");
}

#[test]
fn the_sort_change_reports_once() {
    let mut gallery = Gallery::new(SortMode::Natural);
    assert_eq!(gallery.take_sort_changed(), None);
    gallery.sort_mode = SortMode::Date;
    gallery.sort_changed = true;
    assert_eq!(gallery.take_sort_changed(), Some(SortMode::Date));
    assert_eq!(gallery.take_sort_changed(), None, "the flag clears");
}

#[test]
fn a_sort_change_rebuilds_the_cached_visible_order() {
    let mut gallery = Gallery::new(SortMode::Natural);
    gallery.set_folder(Some(PathBuf::from("/images")));
    gallery.set_items(items());
    assert_eq!(gallery.visible_order(), [3, 2, 1, 0]);

    gallery.set_sort_mode(SortMode::Date);

    assert_eq!(
        gallery.visible_order(),
        [1, 0, 3, 2],
        "the cache followed the sort mode"
    );
}

#[test]
fn set_items_clears_the_selection_when_the_new_list_has_the_same_length() {
    let mut gallery = Gallery::new(SortMode::Natural);
    gallery.set_items(items());
    gallery.selection.select_only(1);

    gallery.set_items(items_in_two_folders());

    assert!(
        gallery.selection.is_empty(),
        "index 1 now names a different file"
    );
}
