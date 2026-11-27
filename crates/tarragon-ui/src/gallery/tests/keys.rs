//! Tests for the select all hook and the selection order.

use super::*;

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
