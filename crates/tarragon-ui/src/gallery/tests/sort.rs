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
fn set_items_drops_a_selected_path_that_left_the_list() {
    let mut gallery = Gallery::new(SortMode::Natural);
    gallery.set_scope(Scope::All);
    gallery.set_items(items());
    gallery.selection.select_only(3);
    let file = |name: &str| GridItem::new(PathBuf::from("/images").join(name), 1, None);
    // a 2.png is gone, and index 3 now holds c.png.
    gallery.set_items(vec![
        file("b.png"),
        file("a.png"),
        file("a 10.png"),
        file("c.png"),
    ]);
    assert!(gallery.selection.is_empty(), "a 2.png is not in the list");
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
fn set_items_does_not_select_another_file_that_takes_the_same_index() {
    let mut gallery = Gallery::new(SortMode::Natural);
    gallery.set_scope(Scope::All);
    gallery.set_items(items());
    gallery.selection.select_only(1);

    gallery.set_items(items_in_two_folders());

    assert!(
        gallery.selection.is_empty(),
        "index 1 now names a different file"
    );
}
