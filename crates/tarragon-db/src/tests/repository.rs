use tarragon_core::color::TRANSPARENT_TAG;
use tarragon_core::entity::TagSource;

use crate::{Database, DbError};

use super::support::{TempDb, file, sized_file};

#[test]
fn schema_creation_is_idempotent() {
    let temp = TempDb::new();
    assert_eq!(temp.db.schema_version().expect("read marker"), 3);

    temp.db.initialize().expect("second create is fine");
    assert_eq!(temp.db.schema_version().expect("read marker"), 3);

    let conn = temp.db.lock().expect("lock");
    let tables: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master \
             WHERE type = 'table' AND name NOT LIKE 'sqlite_%'",
            [],
            |row| row.get(0),
        )
        .expect("count tables");
    assert_eq!(tables, 9, "the schema must hold nine tables");
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
    let (temp, error) = initialize_with_marker(2);
    assert!(
        matches!(
            error,
            crate::DbError::SchemaVersion {
                found: 2,
                expected: 3
            }
        ),
        "got {error:?}"
    );
    assert_eq!(temp.db.schema_version().expect("read marker"), 2);
}

#[test]
fn initialize_refuses_a_newer_marker_and_keeps_it() {
    let (temp, error) = initialize_with_marker(4);
    assert!(
        matches!(
            error,
            crate::DbError::SchemaVersion {
                found: 4,
                expected: 3
            }
        ),
        "got {error:?}"
    );
    assert_eq!(temp.db.schema_version().expect("read marker"), 4);
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
        .ensure_tag("sunset", TagSource::User)
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
fn deleting_the_transparent_tag_is_refused() {
    let temp = TempDb::new();
    let tag = temp
        .db
        .ensure_tag(TRANSPARENT_TAG, TagSource::AutoColor)
        .expect("make the tag");
    let paths = vec!["/refs/a.png".to_owned()];
    temp.db
        .add_tags_to_files_batch(&paths, std::slice::from_ref(&tag))
        .expect("attach the tag");

    let err = temp
        .db
        .delete_tag(tag.id)
        .expect_err("the delete is refused");
    assert!(matches!(err, DbError::ProtectedTag(_)));

    let conn = temp.db.lock().expect("lock");
    let tags: i64 = conn
        .query_row("SELECT COUNT(*) FROM tags WHERE id = ?1", [tag.id], |row| {
            row.get(0)
        })
        .expect("count tags");
    let links: i64 = conn
        .query_row("SELECT COUNT(*) FROM file_tags", [], |row| row.get(0))
        .expect("count file_tags");
    assert_eq!((tags, links), (1, 1), "the tag and its links must remain");
}

#[test]
fn deleting_an_auto_color_tag_is_refused() {
    let temp = TempDb::new();
    let tag = temp
        .db
        .ensure_tag("blue", TagSource::AutoColor)
        .expect("make the tag");
    let paths = vec!["/refs/a.png".to_owned()];
    temp.db
        .add_tags_to_files_batch(&paths, std::slice::from_ref(&tag))
        .expect("attach the tag");

    let err = temp
        .db
        .delete_tag(tag.id)
        .expect_err("the delete is refused");
    assert!(matches!(&err, DbError::ProtectedTag(name) if name == "blue"));

    let conn = temp.db.lock().expect("lock");
    let tags: i64 = conn
        .query_row("SELECT COUNT(*) FROM tags WHERE id = ?1", [tag.id], |row| {
            row.get(0)
        })
        .expect("count tags");
    let links: i64 = conn
        .query_row("SELECT COUNT(*) FROM file_tags", [], |row| row.get(0))
        .expect("count file_tags");
    assert_eq!((tags, links), (1, 1), "the tag and its links must remain");
}

#[test]
fn deleting_a_user_tag_keeps_other_tags_of_its_files() {
    let temp = TempDb::new();
    let doomed = temp
        .db
        .ensure_tag("sunset", TagSource::User)
        .expect("make the tag");
    let kept = temp
        .db
        .ensure_tag("portrait", TagSource::User)
        .expect("make the other tag");
    let paths: Vec<String> = ["a", "b", "c"]
        .iter()
        .map(|stem| format!("/refs/{stem}.png"))
        .collect();
    temp.db
        .add_tags_to_files_batch(&paths, &[doomed.clone(), kept.clone()])
        .expect("attach the tags");

    let changed = temp.db.delete_tag(doomed.id).expect("delete the tag");
    assert_eq!(changed, 1);

    let conn = temp.db.lock().expect("lock");
    let doomed_links: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM file_tags WHERE tag_id = ?1",
            [doomed.id],
            |row| row.get(0),
        )
        .expect("count deleted links");
    let kept_links: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM file_tags WHERE tag_id = ?1",
            [kept.id],
            |row| row.get(0),
        )
        .expect("count kept links");
    assert_eq!((doomed_links, kept_links), (0, 3));
}

#[test]
fn deleting_an_unknown_tag_id_changes_nothing() {
    let temp = TempDb::new();
    assert_eq!(temp.db.delete_tag(9999).expect("delete"), 0);
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
    let sunset = temp
        .db
        .ensure_tag("sunset", TagSource::User)
        .expect("make sunset");
    let harbor = temp
        .db
        .ensure_tag("harbor", TagSource::User)
        .expect("make harbor");
    let paths = vec!["/a.png".to_owned(), "/b.png".to_owned()];
    let pairs = temp
        .db
        .add_tags_to_files_batch(&paths, &[sunset, harbor])
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

/// Write a row with full cache links and a legacy hash.
fn linked_row(temp: &TempDb, path: &str, mtime: i64) {
    temp.db
        .upsert_thumbnail(&crate::repository::ThumbnailWrite {
            path: path.to_owned(),
            mtime,
            size: 500 + mtime,
            width: 40,
            height: 30,
            cache_uuid: "uuid-1".to_owned(),
            thumbnail_cache_path: Some(format!("{path}.t")),
            preview_cache_path: Some(format!("{path}.p")),
            full_cache_path: Some(format!("{path}.f")),
            last_accessed: 900,
        })
        .expect("write the linked row");
    let conn = temp.db.lock().expect("lock");
    conn.execute(
        "UPDATE thumbnails SET thumb_hash = 'h' WHERE path = ?1",
        [path],
    )
    .expect("set the hash");
}

fn table_count(temp: &TempDb, table: &str) -> i64 {
    let conn = temp.db.lock().expect("lock");
    conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
        row.get(0)
    })
    .expect("count rows")
}

#[test]
fn clear_cache_links_resets_links_and_keeps_file_rows() {
    let temp = TempDb::new();
    let paths = ["/refs/a.png", "/refs/b.png", "/refs/c.png"];
    for (mtime, path) in (1..).zip(paths) {
        linked_row(&temp, path, mtime);
    }
    let before: Vec<_> = paths
        .iter()
        .map(|p| temp.db.thumbnail(p).expect("read").expect("row"))
        .collect();

    let changed = temp.db.clear_cache_links().expect("clear the links");

    assert_eq!(changed, 3, "every linked row must count");
    assert_eq!(temp.db.count_thumbnails().expect("count"), 3);
    for old in before {
        let row = temp.db.thumbnail(&old.path).expect("read").expect("row");
        assert_eq!(row.thumb_hash, None);
        assert_eq!(row.cache_uuid, None);
        assert_eq!(row.thumbnail_cache_path, None);
        assert_eq!(row.preview_cache_path, None);
        assert_eq!(row.full_cache_path, None);
        assert_eq!(row.last_accessed, 0);
        assert_eq!(row.mtime, old.mtime);
        assert_eq!(row.size, old.size);
        assert_eq!(row.width, old.width);
        assert_eq!(row.height, old.height);
        assert_eq!(row.created_at, old.created_at);
    }
}

#[test]
fn clear_cache_links_twice_changes_nothing_the_second_time() {
    let temp = TempDb::new();
    linked_row(&temp, "/refs/a.png", 1);
    temp.db
        .write_scan_group(&[sized_file("/refs", "stub.png", 2, 20, 8, 6)])
        .expect("write the stub");
    assert_eq!(temp.db.clear_cache_links().expect("first clear"), 1);
    let after_first: Vec<_> = ["/refs/a.png", "/refs/stub.png"]
        .iter()
        .map(|p| temp.db.thumbnail(p).expect("read").expect("row"))
        .collect();

    let changed = temp.db.clear_cache_links().expect("second clear");

    assert_eq!(changed, 0, "rows at the scan state must not count");
    let after_second: Vec<_> = ["/refs/a.png", "/refs/stub.png"]
        .iter()
        .map(|p| temp.db.thumbnail(p).expect("read").expect("row"))
        .collect();
    assert_eq!(after_first, after_second);
}

#[test]
fn clear_cache_links_leaves_tags_and_folder_uuids() {
    let temp = TempDb::new();
    linked_row(&temp, "/refs/a.png", 1);
    let tag = temp
        .db
        .ensure_tag("sketch", TagSource::User)
        .expect("make the tag");
    temp.db
        .add_tags_to_files_batch(&["/refs/a.png".to_owned()], &[tag])
        .expect("tag the file");
    temp.db
        .upsert_folder_uuid("/refs", "uuid-1")
        .expect("store the folder uuid");
    let counts = |t: &TempDb| {
        (
            table_count(t, "file_tags"),
            table_count(t, "tags"),
            table_count(t, "folder_cache_uuids"),
        )
    };
    let before = counts(&temp);
    assert_eq!(before, (1, 1, 1));

    temp.db.clear_cache_links().expect("clear the links");

    assert_eq!(counts(&temp), before);
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

/// Count the rows in `tags`.
fn tag_rows(temp: &TempDb) -> i64 {
    let conn = temp.db.lock().expect("lock");
    conn.query_row("SELECT COUNT(*) FROM tags", [], |row| row.get(0))
        .expect("count tags")
}

#[test]
fn user_tags_with_reserved_names_are_refused_and_add_no_row() {
    let temp = TempDb::new();
    for name in ["red", "Red", "transparent"] {
        let err = temp
            .db
            .ensure_tag(name, TagSource::User)
            .expect_err("a reserved name is refused");
        assert!(
            matches!(&err, DbError::ReservedTagName(got) if got == name),
            "got {err:?}"
        );
        assert_eq!(err.to_string(), format!("tag name {name} is reserved"));
    }
    assert_eq!(tag_rows(&temp), 0);
}

#[test]
fn auto_color_tags_may_use_reserved_names() {
    let temp = TempDb::new();
    let tag = temp
        .db
        .ensure_tag("red", TagSource::AutoColor)
        .expect("auto tag");
    assert_eq!(tag.source, TagSource::AutoColor);
    temp.db
        .replace_auto_color_tags(
            "/refs/a.png",
            &["blue".to_owned(), TRANSPARENT_TAG.to_owned()],
        )
        .expect("replace the auto tags");
    assert_eq!(tag_rows(&temp), 3);
}

#[test]
fn user_tag_named_like_a_reserved_name_prefix_is_allowed() {
    let temp = TempDb::new();
    let tag = temp
        .db
        .ensure_tag("reddish", TagSource::User)
        .expect("plain name");
    assert_eq!(tag.name, "reddish");
}

#[test]
fn batched_write_takes_a_reserved_auto_tag_unchanged() {
    let temp = TempDb::new();
    let tag = temp
        .db
        .ensure_tag("red", TagSource::AutoColor)
        .expect("auto tag");
    let paths = vec!["/a.png".to_owned()];
    let pairs = temp
        .db
        .add_tags_to_files_batch(&paths, &[tag])
        .expect("attach");
    assert_eq!(pairs, 1);
}
