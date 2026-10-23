//! Tests for the name filter.

use super::*;

#[test]
fn the_visible_filter_waits_for_the_window() {
    let mut gallery = Gallery::new(SortMode::Natural);
    gallery.set_folder(Some(PathBuf::from("/images")));
    gallery.set_items(items());
    let start = Instant::now();
    gallery.search = "png".to_owned();
    gallery.debouncer.arm(start);

    gallery.apply_pending_search(start);
    assert_eq!(gallery.visible_len(), 4, "the filter waits");

    gallery.apply_pending_search(start + SEARCH_DEBOUNCE);
    assert_eq!(gallery.visible_len(), 4, "every name holds png");
}

#[test]
fn the_debounce_narrows_the_visible_set_after_the_window() {
    let mut gallery = Gallery::new(SortMode::Natural);
    gallery.set_folder(Some(PathBuf::from("/images")));
    gallery.set_items(items());
    let start = Instant::now();
    gallery.search = "a 2".to_owned();
    gallery.debouncer.arm(start);

    gallery.apply_pending_search(start + Duration::from_millis(299));
    assert_eq!(gallery.visible_len(), 4, "before the deadline");

    gallery.apply_pending_search(start + SEARCH_DEBOUNCE);
    assert_eq!(gallery.visible_len(), 1, "only a 2.png matches");
}

#[test]
fn set_items_refreshes_the_cached_name() {
    let mut gallery = Gallery::new(SortMode::Natural);
    let mut item = GridItem::new(PathBuf::from("/images/a.png"), 1, None);
    item.path = PathBuf::from("/images/B.PNG");
    gallery.set_folder(Some(PathBuf::from("/images")));
    gallery.set_items(vec![item]);
    search(&mut gallery, "b.png");
    assert_eq!(gallery.visible_len(), 1, "the cache followed the new path");
}

#[test]
fn search_matches_the_file_name_ignoring_case() {
    let mut gallery = Gallery::new(SortMode::Natural);
    gallery.set_folder(Some(PathBuf::from("/images")));
    gallery.set_items(items());
    search(&mut gallery, "A.PNG");
    assert_eq!(gallery.visible_len(), 1, "only a.png matches");
}

#[test]
fn search_uses_the_text_without_trim() {
    let mut gallery = Gallery::new(SortMode::Natural);
    gallery.set_folder(Some(PathBuf::from("/images")));
    gallery.set_items(items());
    search(&mut gallery, " a");
    assert_eq!(gallery.visible_len(), 0, "no name holds a leading space");
    search(&mut gallery, "a ");
    assert_eq!(gallery.visible_len(), 2, "the two numbered names match");
}

#[test]
fn a_whitespace_only_search_is_one_active_filter() {
    let mut gallery = Gallery::new(SortMode::Natural);
    gallery.set_folder(Some(PathBuf::from("/images")));
    gallery.set_items(items());
    search(&mut gallery, "   ");
    assert_eq!(gallery.visible_len(), 0);
}

#[test]
fn a_search_that_narrows_the_view_prunes_the_selection_at_once() {
    let mut gallery = Gallery::new(SortMode::Natural);
    gallery.set_folder(Some(PathBuf::from("/images")));
    gallery.set_items(items());
    gallery.selection.select_only(0);
    gallery.selection.toggle(3);

    search(&mut gallery, "a 2");

    assert_eq!(gallery.selection.iter().collect::<Vec<_>>(), [3]);
}
