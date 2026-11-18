//! Folder filter tests: the folder is a case-sensitive path range.

use std::collections::BTreeSet;
use std::path::PathBuf;

use tarragon_core::filter_state::FilterState;

use super::support::{TempDb, file};
use crate::like::folder_range;

/// A filter state with the given folder chips.
fn folders(names: &[&str]) -> FilterState {
    FilterState {
        folders: names.iter().map(PathBuf::from).collect::<BTreeSet<_>>(),
        ..FilterState::default()
    }
}

/// Run the filter engine and return the stored paths.
fn paths(temp: &TempDb, filters: &FilterState) -> Vec<String> {
    temp.db
        .filter_thumbnails(filters)
        .expect("run the filter query")
        .into_iter()
        .map(|row| row.path)
        .collect()
}

#[test]
fn folder_range_trims_slashes_and_normalizes_separators() {
    let expected = ("/a/Pics/".to_owned(), "/a/Pics0".to_owned());
    assert_eq!(folder_range("/a/Pics"), expected);
    assert_eq!(folder_range("/a/Pics/"), expected);
    assert_eq!(folder_range("\\a\\Pics\\"), expected);
}

#[test]
fn folder_range_of_the_root_covers_every_absolute_path() {
    assert_eq!(folder_range("/"), ("/".to_owned(), "0".to_owned()));
}

#[test]
fn filter_folder_is_case_sensitive() {
    let temp = TempDb::new();
    temp.db
        .write_scan_group(&[
            file("/a/Pics", "x.png", 1, 10),
            file("/a/pics", "y.png", 2, 10),
            file("/a/Pics/sub", "z.png", 3, 10),
        ])
        .expect("write the rows");
    let mut found = paths(&temp, &folders(&["/a/Pics"]));
    found.sort();
    assert_eq!(found, ["/a/Pics/sub/z.png", "/a/Pics/x.png"]);
    assert_eq!(paths(&temp, &folders(&["/a/pics"])), ["/a/pics/y.png"]);
}

#[test]
fn filter_folder_excludes_names_that_extend_it() {
    let temp = TempDb::new();
    temp.db
        .write_scan_group(&[
            file("/a/Pics", "x.png", 1, 10),
            file("/a/Pics2", "q.png", 2, 10),
            file("/a", "Pics.png", 3, 10),
        ])
        .expect("write the rows");
    assert_eq!(paths(&temp, &folders(&["/a/Pics"])), ["/a/Pics/x.png"]);
}

#[test]
fn filter_folder_with_special_characters_matches_itself_only() {
    let temp = TempDb::new();
    temp.db
        .write_scan_group(&[
            file("/a/10%", "p.png", 1, 10),
            file("/a/10x", "px.png", 2, 10),
            file("/a/m_n", "u.png", 3, 10),
            file("/a/mXn", "ux.png", 4, 10),
            file("/a/b\\c", "s.png", 5, 10),
            file("/a/bXc", "sx.png", 6, 10),
        ])
        .expect("write the rows");
    assert_eq!(paths(&temp, &folders(&["/a/10%"])), ["/a/10%/p.png"]);
    assert_eq!(paths(&temp, &folders(&["/a/m_n"])), ["/a/m_n/u.png"]);
    // A backslash is a separator in both the stored path and the folder.
    assert_eq!(paths(&temp, &folders(&["/a/b\\c"])), ["/a/b/c/s.png"]);
}

#[test]
fn filter_folders_return_the_union_and_ignore_a_trailing_slash() {
    let temp = TempDb::new();
    temp.db
        .write_scan_group(&[
            file("/a", "one.png", 1, 10),
            file("/b", "two.png", 2, 10),
            file("/c", "three.png", 3, 10),
        ])
        .expect("write the rows");
    assert_eq!(
        paths(&temp, &folders(&["/a/", "/b"])),
        ["/a/one.png", "/b/two.png"]
    );
}

#[test]
fn filter_root_folder_returns_every_absolute_path() {
    let temp = TempDb::new();
    temp.db
        .write_scan_group(&[file("", "top.png", 1, 10), file("/a/b", "deep.png", 2, 10)])
        .expect("write the rows");
    assert_eq!(
        paths(&temp, &folders(&["/"])),
        ["/a/b/deep.png", "/top.png"]
    );
}

#[test]
fn filter_folder_compares_non_ascii_names_bytewise() {
    let temp = TempDb::new();
    temp.db
        .write_scan_group(&[
            file("/a/\u{e9}t\u{e9}", "low.png", 1, 10),
            file("/a/\u{c9}t\u{e9}", "up.png", 2, 10),
        ])
        .expect("write the rows");
    assert_eq!(
        paths(&temp, &folders(&["/a/\u{e9}t\u{e9}"])),
        ["/a/\u{e9}t\u{e9}/low.png"]
    );
}

#[test]
fn filter_text_still_ignores_case_inside_a_folder_filter() {
    let temp = TempDb::new();
    temp.db
        .write_scan_group(&[
            file("/a/Pics", "x.png", 1, 10),
            file("/a/pics", "y.png", 2, 10),
        ])
        .expect("write the rows");
    let filters = FilterState {
        text: "PICS".to_owned(),
        ..FilterState::default()
    };
    assert_eq!(paths(&temp, &filters), ["/a/Pics/x.png", "/a/pics/y.png"]);
}
