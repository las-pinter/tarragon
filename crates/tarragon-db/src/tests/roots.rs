//! Library root tests.

use super::support::TempDb;

#[test]
fn fresh_database_has_nine_tables_marker_three_and_no_roots() {
    let temp = TempDb::new();
    assert_eq!(temp.db.schema_version().expect("read marker"), 3);
    let tables: i64 = temp
        .db
        .lock()
        .expect("lock")
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master \
             WHERE type = 'table' AND name NOT LIKE 'sqlite_%'",
            [],
            |row| row.get(0),
        )
        .expect("count tables");
    assert_eq!(tables, 9);
    let roots = temp.db.list_library_roots().expect("list");
    assert_eq!(roots.len(), 0, "a fresh database has no roots");
}

#[test]
fn adding_the_same_root_twice_returns_true_then_false() {
    let temp = TempDb::new();
    assert!(temp.db.add_library_root("/r/a").expect("first add"));
    assert!(!temp.db.add_library_root("/r/a").expect("second add"));
    assert_eq!(temp.db.list_library_roots().expect("list"), ["/r/a"]);
}

#[test]
fn roots_list_in_insertion_order() {
    let temp = TempDb::new();
    for path in ["/r/z", "/r/a", "/r/m"] {
        temp.db.add_library_root(path).expect("add");
    }
    assert_eq!(
        temp.db.list_library_roots().expect("list"),
        ["/r/z", "/r/a", "/r/m"]
    );
}

#[test]
fn a_root_added_after_a_removal_goes_last() {
    let temp = TempDb::new();
    for path in ["/r/z", "/r/y", "/r/x"] {
        temp.db.add_library_root(path).expect("add");
    }
    assert_eq!(temp.db.remove_library_root("/r/z").expect("remove"), 1);
    temp.db.add_library_root("/r/a").expect("add");
    assert_eq!(
        temp.db.list_library_roots().expect("list"),
        ["/r/y", "/r/x", "/r/a"]
    );
}

#[test]
fn removing_a_missing_root_returns_zero() {
    let temp = TempDb::new();
    temp.db.add_library_root("/r/a").expect("add");
    assert_eq!(temp.db.remove_library_root("/r/none").expect("remove"), 0);
    assert_eq!(temp.db.list_library_roots().expect("list").len(), 1);
}

#[test]
fn backslashes_are_stored_as_slashes_and_case_is_kept() {
    let temp = TempDb::new();
    assert!(temp.db.add_library_root("D:\\Art\\Refs").expect("add"));
    assert!(
        temp.db.add_library_root("D:/art/refs").expect("add"),
        "a different case is a different root"
    );
    assert!(!temp.db.add_library_root("D:/Art/Refs").expect("add"));
    assert_eq!(temp.db.remove_library_root("D:\\art\\refs").expect("rm"), 1);
    assert_eq!(temp.db.list_library_roots().expect("list"), ["D:/Art/Refs"]);
}

#[test]
fn a_trailing_slash_does_not_make_a_second_root() {
    let temp = TempDb::new();
    assert!(temp.db.add_library_root("/a/").expect("add"));
    assert!(!temp.db.add_library_root("/a").expect("add"));
    assert_eq!(temp.db.list_library_roots().expect("list"), ["/a"]);
}

#[test]
fn removing_with_a_trailing_slash_removes_the_root() {
    let temp = TempDb::new();
    temp.db.add_library_root("/a").expect("add");
    assert_eq!(temp.db.remove_library_root("/a/").expect("remove"), 1);
    assert_eq!(temp.db.list_library_roots().expect("list").len(), 0);
}

#[test]
fn a_filesystem_root_and_a_bare_drive_keep_their_slash() {
    let temp = TempDb::new();
    temp.db.add_library_root("/").expect("add");
    temp.db.add_library_root("C:\\").expect("add");
    assert_eq!(temp.db.list_library_roots().expect("list"), ["/", "C:/"]);
    assert_eq!(temp.db.remove_library_root("C:/").expect("remove"), 1);
}

#[test]
fn a_non_ascii_root_round_trips() {
    let temp = TempDb::new();
    temp.db
        .add_library_root("/r/Bilder/Müller 日本")
        .expect("add");
    assert_eq!(
        temp.db.list_library_roots().expect("list"),
        ["/r/Bilder/Müller 日本"]
    );
}
