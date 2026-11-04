//! Folder listing and folder delete tests: the folder is a case-sensitive
//! path range.

use tarragon_core::entity::TagSource;

use super::support::{TempDb, file};

/// List the direct children of a folder as sorted stored paths.
fn listed(temp: &TempDb, folder: &str) -> Vec<String> {
    let mut paths: Vec<String> = temp
        .db
        .thumbnails_for_folder(folder)
        .expect("list the folder")
        .into_iter()
        .map(|row| row.path)
        .collect();
    paths.sort();
    paths
}

/// Every stored path, sorted.
fn all_paths(temp: &TempDb) -> Vec<String> {
    let mut paths: Vec<String> = temp
        .db
        .library_files()
        .expect("read the rows")
        .into_iter()
        .map(|row| row.path)
        .collect();
    paths.sort();
    paths
}

#[test]
fn listing_is_case_sensitive_and_direct_children_only() {
    let temp = TempDb::new();
    temp.db
        .write_scan_group(&[
            file("/a/Pics", "x.png", 1, 10),
            file("/a/pics", "y.png", 2, 10),
            file("/a/Pics/sub", "z.png", 3, 10),
        ])
        .expect("write the rows");
    assert_eq!(listed(&temp, "/a/Pics"), ["/a/Pics/x.png"]);
    assert_eq!(listed(&temp, "/a/pics"), ["/a/pics/y.png"]);
}

#[test]
fn listing_a_name_with_wildcards_returns_its_own_children() {
    let temp = TempDb::new();
    temp.db
        .write_scan_group(&[
            file("/a/10%", "p.png", 1, 10),
            file("/a/10x", "px.png", 2, 10),
            file("/a/m_n", "u.png", 3, 10),
            file("/a/mXn", "ux.png", 4, 10),
        ])
        .expect("write the rows");
    assert_eq!(listed(&temp, "/a/10%"), ["/a/10%/p.png"]);
    assert_eq!(listed(&temp, "/a/m_n"), ["/a/m_n/u.png"]);
}

#[test]
fn listing_ignores_a_trailing_slash_and_a_backslash_separator() {
    let temp = TempDb::new();
    temp.db
        .write_scan_group(&[file("/a/Pics", "x.png", 1, 10)])
        .expect("write the row");
    let expected = ["/a/Pics/x.png"];
    assert_eq!(listed(&temp, "/a/Pics/"), expected);
    assert_eq!(listed(&temp, "\\a\\Pics"), expected);
    assert_eq!(listed(&temp, "\\a\\Pics\\"), expected);
}

#[test]
fn listing_the_root_returns_top_level_files_only() {
    let temp = TempDb::new();
    temp.db
        .write_scan_group(&[file("", "top.png", 1, 10), file("/a", "x.png", 2, 10)])
        .expect("write the rows");
    assert_eq!(listed(&temp, "/"), ["/top.png"]);
}

#[test]
fn listing_a_non_ascii_folder_counts_characters_not_bytes() {
    let temp = TempDb::new();
    temp.db
        .write_scan_group(&[
            file("/a/\u{e9}t\u{e9}", "low.png", 1, 10),
            file("/a/\u{e9}t\u{e9}/\u{4e2d}", "deep.png", 2, 10),
            file("/a/\u{c9}t\u{e9}", "up.png", 3, 10),
            file("/\u{4e2d}\u{6587}", "cjk.png", 4, 10),
            file("/\u{4e2d}\u{6587}/ab", "c.png", 5, 10),
        ])
        .expect("write the rows");
    assert_eq!(
        listed(&temp, "/a/\u{e9}t\u{e9}"),
        ["/a/\u{e9}t\u{e9}/low.png"]
    );
    assert_eq!(
        listed(&temp, "/\u{4e2d}\u{6587}"),
        ["/\u{4e2d}\u{6587}/cjk.png"]
    );
}

#[test]
fn delete_by_folder_keeps_case_and_name_siblings_and_all_tags() {
    let temp = TempDb::new();
    temp.db
        .write_scan_group(&[
            file("/a/Pics", "x.png", 1, 10),
            file("/a/Pics/sub", "z.png", 2, 10),
            file("/a/pics", "y.png", 3, 10),
            file("/a/pics/sub", "w.png", 4, 10),
            file("/a/Pics2", "q.png", 5, 10),
            file("/a", "Pics.png", 6, 10),
        ])
        .expect("write the rows");
    let paths = all_paths(&temp);
    let tag = temp.db.ensure_tag("ref", TagSource::User).expect("tag");
    temp.db
        .add_tags_to_files_batch(&paths, &[tag])
        .expect("tag every row");
    let before = temp.db.tags_for_files(&paths).expect("read the tags");

    let removed = temp
        .db
        .delete_thumbnails_by_folder("/a/Pics")
        .expect("delete the folder");

    assert_eq!(removed, 2);
    assert_eq!(
        all_paths(&temp),
        [
            "/a/Pics.png",
            "/a/Pics2/q.png",
            "/a/pics/sub/w.png",
            "/a/pics/y.png"
        ]
    );
    assert_eq!(
        temp.db.tags_for_files(&paths).expect("read the tags"),
        before
    );
}
