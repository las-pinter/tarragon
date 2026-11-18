//! Tests for the database filter result and stale folder chips.

use super::*;
use std::collections::HashSet;

fn gallery() -> Gallery {
    let mut gallery = Gallery::new(SortMode::Natural);
    gallery.set_scope(Scope::All);
    gallery.set_items(items_in_two_folders());
    gallery
}

fn set_of(names: &[&str]) -> HashSet<PathBuf> {
    names.iter().map(PathBuf::from).collect()
}

#[test]
fn a_set_hides_items_outside_it_and_none_shows_all() {
    let mut gallery = gallery();
    gallery.set_filter_result(Some(set_of(&["/library/renders/c.png"])));
    assert_eq!(gallery.visible_len(), 1);
    gallery.set_filter_result(None);
    assert_eq!(gallery.visible_len(), 4);
}

#[test]
fn the_set_combines_with_search_and_scope() {
    let mut gallery = gallery();
    let both = ["/library/sketches/a.png", "/library/renders/c.png"];
    gallery.set_filter_result(Some(set_of(&both)));
    search(&mut gallery, "a.png");
    assert_eq!(gallery.visible_len(), 1);
    gallery.set_scope(Scope::Folder);
    gallery.set_folder(Some(PathBuf::from("/library/renders")));
    assert_eq!(gallery.visible_len(), 0);
    // Search and scope both pass sketches/a.png. Only the set excludes it.
    gallery.set_folder(Some(sketches()));
    assert_eq!(gallery.visible_len(), 1);
    gallery.set_filter_result(Some(set_of(&["/library/renders/c.png"])));
    assert_eq!(gallery.visible_len(), 0);
}

#[test]
fn a_selected_item_that_becomes_hidden_leaves_the_selection() {
    let mut gallery = gallery();
    for index in gallery.visible.clone() {
        gallery.selection.toggle(index);
    }
    assert_eq!(gallery.selected_paths().len(), 4);
    gallery.set_filter_result(Some(set_of(&["/library/renders/d.png"])));
    assert_eq!(
        gallery.selected_paths(),
        [PathBuf::from("/library/renders/d.png")]
    );
}

#[test]
fn the_info_bar_count_matches_the_visible_count() {
    let mut gallery = gallery();
    gallery.set_filter_result(Some(set_of(&["/library/sketches/b.png"])));
    assert_eq!(gallery.visible_len(), 1);
    assert_eq!(gallery.info_text(), "All Images - 1 file");
}

#[test]
fn a_stale_folder_chip_goes_and_flags_a_change() {
    let mut gallery = gallery();
    gallery.bar.toggle_folder(Path::new("/library/sketches"));
    gallery.bar.toggle_folder(Path::new("/library/gone"));
    gallery.set_filter_lists(Vec::new(), vec!["/library/sketches".into()]);
    let (state, _) = gallery.take_filter_change().expect("a change");
    assert_eq!(state.folders.len(), 1);
    assert_eq!(gallery.bar.folder_order(), [sketches()]);
    gallery.set_filter_lists(Vec::new(), vec!["/library/sketches".into()]);
    assert_eq!(gallery.take_filter_change(), None);
}
