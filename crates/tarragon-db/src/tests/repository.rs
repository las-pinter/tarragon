use tarragon_core::entity::TagSource;

use crate::Database;

use super::support::{TempDb, file, sized_file};

#[test]
fn schema_creation_is_idempotent() {
    let temp = TempDb::new();
    assert_eq!(temp.db.schema_version().expect("read marker"), 2);

    temp.db.initialize().expect("second create is fine");
    assert_eq!(temp.db.schema_version().expect("read marker"), 2);

    let conn = temp.db.lock().expect("lock");
    let tables: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master \
             WHERE type = 'table' AND name NOT LIKE 'sqlite_%'",
            [],
            |row| row.get(0),
        )
        .expect("count tables");
    assert_eq!(tables, 8, "the schema must hold eight tables");
    let marker_rows: i64 = conn
        .query_row("SELECT COUNT(*) FROM schema_version", [], |row| row.get(0))
        .expect("count marker rows");
    assert_eq!(marker_rows, 1, "the marker must hold one row");
}

/// Store a marker value, then run `initialize` and return its error.
fn initialize_with_marker(marker: i64) -> (TempDb, crate::DbError) {
    let temp = TempDb::new();
    {
        let conn = temp.db.lock().expect("lock");
        conn.execute("UPDATE schema_version SET version = ?1", [marker])
            .expect("store the marker");
    }
    let error = temp.db.initialize().expect_err("a mismatch must fail");
    (temp, error)
}

#[test]
fn initialize_refuses_an_older_marker_and_keeps_it() {
    let (temp, error) = initialize_with_marker(1);
    assert!(
        matches!(
            error,
            crate::DbError::SchemaVersion {
                found: 1,
                expected: 2
            }
        ),
        "got {error:?}"
    );
    assert_eq!(temp.db.schema_version().expect("read marker"), 1);
}

#[test]
fn initialize_refuses_a_newer_marker_and_keeps_it() {
    let (temp, error) = initialize_with_marker(3);
    assert!(
        matches!(
            error,
            crate::DbError::SchemaVersion {
                found: 3,
                expected: 2
            }
        ),
        "got {error:?}"
    );
    assert_eq!(temp.db.schema_version().expect("read marker"), 3);
}

#[test]
fn wal_journal_mode_is_on() {
    let temp = TempDb::new();
    let conn = temp.db.lock().expect("lock");
    let mode: String = conn
        .query_row("PRAGMA journal_mode", [], |row| row.get(0))
        .expect("read journal mode");
    assert_eq!(mode.to_ascii_lowercase(), "wal");
}

#[test]
fn foreign_keys_are_on() {
    let temp = TempDb::new();
    let conn = temp.db.lock().expect("lock");
    let on: i64 = conn
        .query_row("PRAGMA foreign_keys", [], |row| row.get(0))
        .expect("read foreign_keys");
    assert_eq!(on, 1);
}

#[test]
fn tag_delete_cascades_to_file_tags() {
    let temp = TempDb::new();
    let tag = temp
        .db
        .ensure_tag("red", TagSource::User)
        .expect("make the tag");
    let paths = vec!["/refs/a.png".to_owned()];
    temp.db
        .add_tags_to_files_batch(&paths, std::slice::from_ref(&tag))
        .expect("attach the tag");

    let before = temp.db.tags_for_files(&paths).expect("read tags");
    assert_eq!(before["/refs/a.png"].len(), 1);

    temp.db.delete_tag(tag.id).expect("delete the tag");

    let conn = temp.db.lock().expect("lock");
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM file_tags", [], |row| row.get(0))
        .expect("count file_tags");
    assert_eq!(count, 0, "the foreign key must cascade the delete");
}

#[test]
fn thumbnails_have_a_last_accessed_column() {
    let temp = TempDb::new();
    temp.db
        .write_scan_group(&[file("/refs", "a.png", 10, 100)])
        .expect("write the scan group");

    let conn = temp.db.lock().expect("lock");
    let present: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('thumbnails') WHERE name = 'last_accessed'",
            [],
            |row| row.get(0),
        )
        .expect("read the table info");
    assert_eq!(present, 1, "thumbnails.last_accessed must exist");
    drop(conn);

    let changed = temp
        .db
        .touch_last_accessed(&["/refs/a.png".to_owned()], 42)
        .expect("touch the row");
    assert_eq!(changed, 1);
    let thumb = temp
        .db
        .thumbnail("/refs/a.png")
        .expect("read the row")
        .expect("the row exists");
    assert_eq!(thumb.last_accessed, 42);
}

#[test]
fn thumbnails_are_listed_in_last_accessed_order() {
    let temp = TempDb::new();
    let files = [
        file("/refs", "a.png", 1, 10),
        file("/refs", "b.png", 1, 10),
        file("/refs", "c.png", 1, 10),
    ];
    temp.db.write_scan_group(&files).expect("write the rows");
    temp.db
        .touch_last_accessed(&["/refs/a.png".to_owned()], 30)
        .expect("touch a");
    temp.db
        .touch_last_accessed(&["/refs/b.png".to_owned()], 10)
        .expect("touch b");
    temp.db
        .touch_last_accessed(&["/refs/c.png".to_owned()], 20)
        .expect("touch c");

    let ordered = temp
        .db
        .thumbnails_by_last_accessed()
        .expect("list the rows");
    let paths: Vec<&str> = ordered.iter().map(|row| row.path.as_str()).collect();
    assert_eq!(
        paths,
        vec!["/refs/b.png", "/refs/c.png", "/refs/a.png"],
        "the list must run least recently accessed first"
    );
}

#[test]
fn scan_group_rows_are_visible_to_another_connection() {
    let temp = TempDb::new();
    let files = [
        file("/refs", "a.png", 1, 10),
        file("/refs", "b.png", 2, 20),
        file("/refs", "c.png", 3, 30),
    ];
    let written = temp.db.write_scan_group(&files).expect("write the group");
    assert_eq!(written, 3);
    assert_eq!(temp.db.count_thumbnails().expect("count"), 3);

    let other = Database::open(temp.path()).expect("open a second connection");
    assert_eq!(
        other.count_thumbnails().expect("count on the reader"),
        3,
        "a committed single transaction is visible to another connection"
    );
}

#[test]
fn scan_group_rolls_back_as_one_transaction() {
    let temp = TempDb::new();
    {
        let conn = temp.db.lock().expect("lock");
        conn.execute_batch(
            "CREATE TRIGGER fail_b BEFORE INSERT ON thumbnails \
             WHEN NEW.path = '/refs/b.png' BEGIN SELECT RAISE(ABORT, 'boom'); END;",
        )
        .expect("make the failing trigger");
    }

    let files = [
        file("/refs", "a.png", 1, 10),
        file("/refs", "b.png", 2, 20),
        file("/refs", "c.png", 3, 30),
    ];
    let result = temp.db.write_scan_group(&files);
    assert!(result.is_err(), "the batch must fail");
    assert_eq!(
        temp.db.count_thumbnails().expect("count"),
        0,
        "one bad row must roll back the whole group"
    );
}

#[test]
fn scan_group_paths_use_forward_slashes() {
    let temp = TempDb::new();
    temp.db
        .write_scan_group(&[file(r"C:\refs", "a.png", 1, 10)])
        .expect("write the group");
    let thumb = temp
        .db
        .thumbnail("C:/refs/a.png")
        .expect("read the row")
        .expect("the row exists");
    assert_eq!(thumb.path, "C:/refs/a.png");
}

#[test]
fn scan_group_keeps_the_stored_mtime_and_size() {
    let temp = TempDb::new();
    temp.db
        .write_scan_group(&[file("/refs", "a.png", 10, 100)])
        .expect("first write");
    temp.db
        .write_scan_group(&[file("/refs", "a.png", 99, 999)])
        .expect("second write");
    let thumb = temp
        .db
        .thumbnail("/refs/a.png")
        .expect("read the row")
        .expect("the row exists");
    assert_eq!(thumb.mtime, 10);
    assert_eq!(thumb.size, 100);
}

#[test]
fn batched_tag_write_counts_every_pair() {
    let temp = TempDb::new();
    let red = temp
        .db
        .ensure_tag("red", TagSource::User)
        .expect("make red");
    let blue = temp
        .db
        .ensure_tag("blue", TagSource::User)
        .expect("make blue");
    let paths = vec!["/a.png".to_owned(), "/b.png".to_owned()];
    let pairs = temp
        .db
        .add_tags_to_files_batch(&paths, &[red, blue])
        .expect("attach both tags");
    assert_eq!(pairs, 4);
    for usage in temp.db.all_tags().expect("read the usage") {
        assert_eq!(usage.usage_count, 2);
    }
}

#[test]
fn scan_group_writes_the_scanned_dimensions() {
    let temp = TempDb::new();
    temp.db
        .write_scan_group(&[sized_file("/refs", "a.png", 1, 10, 800, 600)])
        .expect("write the group");
    let thumb = temp
        .db
        .thumbnail("/refs/a.png")
        .expect("read the row")
        .expect("the row exists");
    assert_eq!(thumb.width, 800);
    assert_eq!(thumb.height, 600);
}

#[test]
fn thumbnails_for_folder_returns_direct_children_only() {
    let temp = TempDb::new();
    temp.db
        .write_scan_group(&[
            file("/refs", "direct.png", 1, 10),
            file("/refs/nested", "deep.png", 2, 20),
        ])
        .expect("write the group");

    let rows = temp
        .db
        .thumbnails_for_folder("/refs")
        .expect("list the folder");
    let paths: Vec<String> = rows.into_iter().map(|row| row.path).collect();
    assert_eq!(paths, vec!["/refs/direct.png"]);
}

#[test]
fn thumbnails_for_folder_does_not_match_a_sibling_name() {
    let temp = TempDb::new();
    temp.db
        .write_scan_group(&[
            file("/refs", "a.png", 1, 10),
            file("/refs-old", "b.png", 2, 20),
        ])
        .expect("write the group");

    let rows = temp
        .db
        .thumbnails_for_folder("/refs")
        .expect("list the folder");
    let paths: Vec<String> = rows.into_iter().map(|row| row.path).collect();
    assert_eq!(paths, vec!["/refs/a.png"]);
}

#[test]
fn delete_thumbnails_by_folder_removes_every_descendant() {
    let temp = TempDb::new();
    temp.db
        .write_scan_group(&[
            file("/refs", "a.png", 1, 10),
            file("/refs/nested", "b.png", 2, 20),
            file("/other", "c.png", 3, 30),
        ])
        .expect("write the group");

    let removed = temp
        .db
        .delete_thumbnails_by_folder("/refs")
        .expect("delete the folder");
    assert_eq!(removed, 2);
    assert!(temp.db.thumbnail("/refs/a.png").expect("read").is_none());
    assert!(
        temp.db
            .thumbnail("/refs/nested/b.png")
            .expect("read")
            .is_none()
    );
    assert!(temp.db.thumbnail("/other/c.png").expect("read").is_some());
}

#[test]
fn clear_thumbnails_removes_every_row() {
    let temp = TempDb::new();
    temp.db
        .write_scan_group(&[file("/refs", "a.png", 1, 10), file("/refs", "b.png", 2, 20)])
        .expect("write the rows");
    assert_eq!(temp.db.count_thumbnails().expect("count"), 2);

    let removed = temp.db.clear_thumbnails().expect("clear the table");

    assert_eq!(removed, 2, "the clear must report every removed row");
    assert_eq!(
        temp.db.count_thumbnails().expect("count"),
        0,
        "DELETE FROM thumbnails must empty the table"
    );
}

#[test]
fn clear_thumbnails_on_an_empty_table_removes_nothing() {
    let temp = TempDb::new();

    let removed = temp.db.clear_thumbnails().expect("clear an empty table");

    assert_eq!(removed, 0, "an empty clear must report no removed rows");
    assert_eq!(
        temp.db.count_thumbnails().expect("count"),
        0,
        "an empty clear must leave the table empty"
    );
}

#[test]
fn distinct_folders_lists_every_parent_folder() {
    let temp = TempDb::new();
    temp.db
        .write_scan_group(&[
            file("/refs", "a.png", 1, 10),
            file("/refs/nested", "b.png", 2, 20),
            file("/other", "c.png", 3, 30),
        ])
        .expect("write the group");

    let folders = temp.db.distinct_folders().expect("list the folders");
    assert_eq!(folders, vec!["/other", "/refs", "/refs/nested"]);
}

#[test]
fn tags_for_files_merges_path_chunks() {
    let temp = TempDb::new();
    let tag = temp
        .db
        .ensure_tag("bulk", TagSource::User)
        .expect("make the tag");
    let paths: Vec<String> = (0..1200)
        .map(|index| format!("/refs/img{index}.png"))
        .collect();
    temp.db
        .add_tags_to_files_batch(&paths, std::slice::from_ref(&tag))
        .expect("attach the tag");

    let tags = temp.db.tags_for_files(&paths).expect("read the tags");
    assert_eq!(tags.len(), paths.len());
    assert!(
        tags.values().all(|rows| rows.len() == 1),
        "every path must keep its tag across the chunks"
    );
}

#[test]
fn remove_tags_from_files_covers_every_path_chunk() {
    let temp = TempDb::new();
    let tag = temp
        .db
        .ensure_tag("bulk", TagSource::User)
        .expect("make the tag");
    let paths: Vec<String> = (0..1200)
        .map(|index| format!("/refs/img{index}.png"))
        .collect();
    temp.db
        .add_tags_to_files_batch(&paths, std::slice::from_ref(&tag))
        .expect("attach the tag");

    let removed = temp
        .db
        .remove_tags_from_files(&paths, tag.id)
        .expect("remove the tag");
    assert_eq!(removed, paths.len());
}

#[test]
fn scan_stub_stores_no_cache_uuid() {
    let temp = TempDb::new();
    temp.db
        .write_scan_group(&[file("/refs", "a.png", 1, 10)])
        .expect("write the stub");
    let row = temp
        .db
        .thumbnail("/refs/a.png")
        .expect("read")
        .expect("row");
    assert_eq!(row.cache_uuid, None);
}

#[test]
fn reorder_favorites_places_listed_first_and_leaves_no_ties() {
    let temp = TempDb::new();
    for (index, path) in ["/f/a", "/f/b", "/f/c", "/f/d"].into_iter().enumerate() {
        let order = i64::try_from(index).expect("small index");
        temp.db.add_favorite(path, None, order).expect("add");
    }
    let listed = ["/f/c", "/f/a", "/f/c", "/f/missing"].map(str::to_owned);
    let placed = temp.db.reorder_favorites(&listed).expect("reorder");
    assert_eq!(placed, 2, "a repeat and a missing path are not placed");

    let rows = temp.db.list_favorites().expect("list");
    let paths: Vec<&str> = rows.iter().map(|row| row.path.as_str()).collect();
    assert_eq!(paths, ["/f/c", "/f/a", "/f/b", "/f/d"]);
    let orders: Vec<i64> = rows.iter().map(|row| row.sort_order).collect();
    assert_eq!(orders, [0, 1, 2, 3]);
}
