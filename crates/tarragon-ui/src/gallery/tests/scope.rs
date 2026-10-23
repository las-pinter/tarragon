//! Tests for the scope filter, the info bar, and selection pruning.

use super::*;

#[test]
fn an_empty_gallery_shows_nothing() {
    let gallery = Gallery::new(SortMode::Natural);
    assert_eq!(gallery.visible_len(), 0);
    assert_eq!(gallery.sort_mode(), SortMode::Natural);
}

#[test]
fn the_folder_scope_shows_only_the_folder_items() {
    let mut gallery = Gallery::new(SortMode::Natural);
    gallery.set_items(items_in_two_folders());
    gallery.set_folder(Some(sketches()));
    assert_eq!(gallery.visible_len(), 2, "the folder holds two items");

    gallery.set_scope(Scope::All);
    assert_eq!(gallery.visible_len(), 4, "the global scope holds four");
}

#[test]
fn the_all_scope_yields_more_items_than_the_folder_scope() {
    let mut gallery = Gallery::new(SortMode::Natural);
    gallery.set_items(items_in_two_folders());
    gallery.set_folder(Some(sketches()));

    gallery.set_scope(Scope::All);
    let all = gallery.visible_len();
    gallery.set_scope(Scope::Folder);
    let folder = gallery.visible_len();
    assert!(
        all > folder,
        "All shows {all} items but Folder shows {folder}"
    );
}

#[test]
fn the_folder_scope_without_a_folder_shows_nothing() {
    let mut gallery = Gallery::new(SortMode::Natural);
    gallery.set_items(items_in_two_folders());
    assert_eq!(gallery.visible_len(), 0, "no folder is set");
    assert_eq!(gallery.info_text(), "No folder - 0 files");

    gallery.set_scope(Scope::All);
    assert_eq!(gallery.visible_len(), 4, "the global scope stays full");
}

#[test]
fn the_folder_scope_ignores_a_folder_that_holds_no_item() {
    let mut gallery = Gallery::new(SortMode::Natural);
    gallery.set_items(items_in_two_folders());
    gallery.set_folder(Some(PathBuf::from("/library/other")));
    assert_eq!(gallery.visible_len(), 0);
}

#[test]
fn set_folder_normalizes_the_path_once() {
    // A real folder is needed, because canonicalization reads the file system.
    let base = std::env::temp_dir()
        .canonicalize()
        .expect("the temp dir exists")
        .join(format!("tarragon-ui-folder-{}", std::process::id()));
    let child = base.join("child");
    std::fs::create_dir_all(&child).expect("create the test folder");

    let mut gallery = Gallery::new(SortMode::Natural);
    gallery.set_items(vec![GridItem::new(child.join("a.png"), 1, None)]);
    gallery.set_folder(Some(child.join("..").join("child")));
    let visible = gallery.visible_len();
    std::fs::remove_dir_all(&base).ok();

    assert_eq!(visible, 1, "the unnormalized path still finds the folder");
}

#[test]
fn the_info_bar_names_the_folder_and_the_count() {
    let mut gallery = Gallery::new(SortMode::Natural);
    gallery.set_items(items());
    gallery.set_folder(Some(PathBuf::from("/images")));
    assert_eq!(gallery.info_text(), "images - 4 files");

    gallery.set_scope(Scope::All);
    assert_eq!(gallery.info_text(), "All Images - 4 files");
}

#[test]
fn the_info_bar_adds_the_selection_count() {
    let mut gallery = Gallery::new(SortMode::Natural);
    gallery.set_items(items());
    gallery.set_folder(Some(PathBuf::from("/images")));
    gallery.selection.select_only(1);
    gallery.selection.toggle(2);
    assert_eq!(gallery.info_text(), "images - 4 files - 2 selected");
}

#[test]
fn the_scope_name_falls_back_when_no_folder_exists() {
    let gallery = Gallery::new(SortMode::Natural);
    assert_eq!(gallery.scope_name(), "No folder");
}

#[test]
fn a_tab_switch_prunes_the_selection_to_the_visible_items() {
    let mut gallery = Gallery::new(SortMode::Natural);
    gallery.set_items(items_in_two_folders());
    gallery.set_folder(Some(sketches()));
    gallery.set_scope(Scope::All);
    // Indices 2 and 3 sit in the other folder, so the folder scope hides them.
    gallery.selection.select_only(1);
    gallery.selection.toggle(2);

    gallery.set_scope(Scope::Folder);

    assert_eq!(
        gallery.selection.iter().collect::<Vec<_>>(),
        [1],
        "the hidden item left the selection"
    );
}

#[test]
fn a_nonexistent_item_inside_the_folder_stays_in_scope() {
    // Neither the folder nor the item exists on disk. The item path starts
    // with the folder path, so the item stays in scope.
    let mut gallery = Gallery::new(SortMode::Natural);
    gallery.set_items(vec![GridItem::new(
        PathBuf::from("/no/such/folder/ghost.png"),
        1,
        None,
    )]);
    gallery.set_folder(Some(PathBuf::from("/no/such/folder")));
    assert_eq!(gallery.visible_len(), 1);
}

#[cfg(unix)]
#[test]
fn a_symlinked_image_stays_in_the_folder_that_holds_the_link() {
    let base = std::env::temp_dir()
        .canonicalize()
        .expect("the temp dir exists")
        .join(format!("tarragon-ui-symlink-{}", std::process::id()));
    let folder = base.join("folder");
    let elsewhere = base.join("elsewhere");
    std::fs::create_dir_all(&folder).expect("create the folder");
    std::fs::create_dir_all(&elsewhere).expect("create the target folder");
    let target = elsewhere.join("real.png");
    std::fs::write(&target, b"x").expect("write the target");
    let link = folder.join("link.png");
    std::os::unix::fs::symlink(&target, &link).expect("create the symlink");

    let mut gallery = Gallery::new(SortMode::Natural);
    gallery.set_items(vec![GridItem::new(link, 1, None)]);
    gallery.set_folder(Some(folder));
    let visible = gallery.visible_len();
    std::fs::remove_dir_all(&base).ok();

    assert_eq!(visible, 1, "the link left the folder that holds it");
}

/// A real folder, a symlink to it, and an image inside, under a temp base.
#[cfg(unix)]
fn linked_folder(tag: &str) -> (PathBuf, PathBuf, PathBuf) {
    let base = std::env::temp_dir()
        .canonicalize()
        .expect("the temp dir exists")
        .join(format!("tarragon-ui-{tag}-{}", std::process::id()));
    let real = base.join("real");
    std::fs::create_dir_all(&real).expect("create the folder");
    let link = base.join("link");
    std::os::unix::fs::symlink(&real, &link).expect("create the symlink");
    (base, real, link)
}

#[cfg(unix)]
#[test]
fn a_symlinked_folder_finds_items_under_the_link_path() {
    let (base, _real, link) = linked_folder("folder-link");
    let mut gallery = Gallery::new(SortMode::Natural);
    gallery.set_items(vec![GridItem::new(link.join("a.png"), 1, None)]);
    gallery.set_folder(Some(link));
    let visible = gallery.visible_len();
    std::fs::remove_dir_all(&base).ok();

    assert_eq!(visible, 1, "the link path left the folder scope");
}

#[cfg(unix)]
#[test]
fn a_symlinked_folder_finds_items_under_the_canonical_path() {
    let (base, real, link) = linked_folder("folder-real");
    let mut gallery = Gallery::new(SortMode::Natural);
    gallery.set_items(vec![GridItem::new(real.join("a.png"), 1, None)]);
    gallery.set_folder(Some(link));
    let visible = gallery.visible_len();
    std::fs::remove_dir_all(&base).ok();

    assert_eq!(visible, 1, "the canonical path left the folder scope");
}
