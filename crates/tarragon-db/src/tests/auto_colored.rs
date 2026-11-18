use std::collections::HashSet;

use tarragon_core::entity::TagSource;

use super::support::TempDb;

/// Build the expected set from string slices.
fn set(paths: &[&str]) -> HashSet<String> {
    paths.iter().map(|p| (*p).to_owned()).collect()
}

/// Attach the user tag `name` to `path`.
fn user_tag(temp: &TempDb, path: &str, name: &str) {
    let tag = temp.db.ensure_tag(name, TagSource::User).expect("tag");
    temp.db
        .add_tags_to_files_batch(&[path.to_owned()], &[tag])
        .expect("attach");
}

#[test]
fn only_paths_with_an_auto_color_tag_are_listed() {
    let temp = TempDb::new();
    let red = ["red".to_owned()];
    temp.db
        .replace_auto_color_tags("/refs/a.png", &red)
        .expect("tag a");
    temp.db
        .replace_auto_color_tags("/refs/b.png", &red)
        .expect("tag b");
    user_tag(&temp, "/refs/c.png", "mine");
    let got = temp.db.auto_colored_paths().expect("read");
    assert_eq!(got, set(&["/refs/a.png", "/refs/b.png"]));
}

#[test]
fn a_path_with_user_and_auto_tags_is_listed_once() {
    let temp = TempDb::new();
    user_tag(&temp, "/refs/a.png", "mine");
    temp.db
        .replace_auto_color_tags("/refs/a.png", &["red".to_owned(), "blue".to_owned()])
        .expect("tag");
    let got = temp.db.auto_colored_paths().expect("read");
    assert_eq!(got, set(&["/refs/a.png"]));
}

#[test]
fn clearing_the_auto_tags_removes_the_path() {
    let temp = TempDb::new();
    temp.db
        .replace_auto_color_tags("/refs/a.png", &["red".to_owned()])
        .expect("tag a");
    temp.db
        .replace_auto_color_tags("/refs/b.png", &["red".to_owned()])
        .expect("tag b");
    temp.db
        .replace_auto_color_tags("/refs/a.png", &[])
        .expect("clear");
    let got = temp.db.auto_colored_paths().expect("read");
    assert_eq!(got, set(&["/refs/b.png"]));
}

#[test]
fn an_empty_database_gives_an_empty_set() {
    let temp = TempDb::new();
    assert!(temp.db.auto_colored_paths().expect("read").is_empty());
}
