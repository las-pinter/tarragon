//! Tests for the empty card rule.

use super::*;
use std::collections::HashSet;
use tarragon_core::color::ColorBucket;
use tarragon_core::filter_bar::ColorChoice;

/// A gallery in the folder scope with four items in `/images`.
fn filled() -> Gallery {
    let mut gallery = Gallery::new(SortMode::Natural);
    gallery.set_folder(Some(PathBuf::from("/images")));
    gallery.set_items(items());
    gallery
}

#[test]
fn a_new_gallery_has_no_folder_and_loading_hides_the_card() {
    let mut gallery = Gallery::new(SortMode::Natural);
    assert_eq!(gallery.empty_kind(), Some(EmptyKind::NoFolder));
    gallery.set_loading(true);
    assert_eq!(gallery.empty_kind(), None);
    gallery.set_loading(false);
    assert_eq!(gallery.empty_kind(), Some(EmptyKind::NoFolder));
}

#[test]
fn a_folder_without_items_is_an_empty_folder() {
    let mut gallery = Gallery::new(SortMode::Natural);
    gallery.set_folder(Some(PathBuf::from("/images")));
    assert_eq!(gallery.empty_kind(), Some(EmptyKind::EmptyFolder));
}

#[test]
fn a_folder_with_items_outside_it_is_an_empty_folder() {
    let mut gallery = Gallery::new(SortMode::Natural);
    gallery.set_folder(Some(PathBuf::from("/other")));
    gallery.set_items(items());
    assert_eq!(gallery.empty_kind(), Some(EmptyKind::EmptyFolder));
}

#[test]
fn a_search_that_matches_nothing_is_no_result() {
    let mut gallery = filled();
    search(&mut gallery, "zzz");
    assert_eq!(gallery.empty_kind(), Some(EmptyKind::NoResult));
}

#[test]
fn an_empty_filter_result_is_no_result() {
    let mut gallery = filled();
    gallery.set_filter_result(Some(HashSet::new()));
    assert_eq!(gallery.empty_kind(), Some(EmptyKind::NoResult));
}

#[test]
fn a_colour_chip_alone_is_no_result() {
    let mut gallery = Gallery::new(SortMode::Natural);
    gallery.set_folder(Some(PathBuf::from("/images")));
    gallery
        .bar
        .toggle_color(ColorChoice::Bucket(ColorBucket::Red));
    assert_eq!(gallery.empty_kind(), Some(EmptyKind::NoResult));
}

#[test]
fn a_transparent_chip_alone_is_no_result() {
    let mut gallery = Gallery::new(SortMode::Natural);
    gallery.set_folder(Some(PathBuf::from("/images")));
    gallery.bar.state.transparent = true;
    assert_eq!(gallery.empty_kind(), Some(EmptyKind::NoResult));
}

#[test]
fn a_tag_chip_alone_is_no_result() {
    let mut gallery = Gallery::new(SortMode::Natural);
    gallery.set_folder(Some(PathBuf::from("/images")));
    gallery.bar.toggle_tag("ref");
    assert_eq!(gallery.empty_kind(), Some(EmptyKind::NoResult));
}

#[test]
fn a_folder_chip_in_scope_all_is_no_result() {
    let mut gallery = Gallery::new(SortMode::Natural);
    gallery.set_scope(Scope::All);
    gallery.bar.toggle_folder(Path::new("/images"));
    assert_eq!(gallery.empty_kind(), Some(EmptyKind::NoResult));
}

#[test]
fn a_stale_folder_chip_in_folder_scope_does_not_filter() {
    let mut gallery = Gallery::new(SortMode::Natural);
    gallery.set_folder(Some(PathBuf::from("/images")));
    gallery.bar.toggle_folder(Path::new("/images"));
    assert_eq!(gallery.empty_kind(), Some(EmptyKind::EmptyFolder));
}

#[test]
fn scope_all_without_items_is_no_folder_and_with_a_search_is_no_result() {
    let mut gallery = Gallery::new(SortMode::Natural);
    gallery.set_scope(Scope::All);
    assert_eq!(gallery.empty_kind(), Some(EmptyKind::NoFolder));
    search(&mut gallery, "a");
    assert_eq!(gallery.empty_kind(), Some(EmptyKind::NoResult));
}

#[test]
fn a_visible_item_gives_no_card_in_every_scope() {
    let mut gallery = filled();
    assert_eq!(gallery.empty_kind(), None);
    gallery.set_scope(Scope::All);
    assert_eq!(gallery.empty_kind(), None);
}
