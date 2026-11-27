//! Tests for the folder that opens, which the header names early.

use super::*;

#[test]
fn the_loading_folder_names_the_header_until_it_ends() {
    let mut gallery = Gallery::new(SortMode::Natural);
    gallery.set_folder(Some(PathBuf::from("/images")));
    gallery.set_items(items());
    gallery.set_loading_folder(Some(PathBuf::from("/other")));
    assert_eq!(gallery.scope_name(), "other");
    gallery.set_loading_folder(None);
    assert_eq!(gallery.scope_name(), "images");
}
