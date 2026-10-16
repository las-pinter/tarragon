//! Tests for the schema and the repositories.
//!
//! Every test uses a temporary database file. WAL needs a file, so the tests
//! do not use an in-memory database.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use tarragon_core::entity::{FileInfo, TagSource};

use crate::Database;

/// A temporary database file that removes itself on drop.
struct TempDb {
    dir: PathBuf,
    db: Database,
}

impl TempDb {
    /// Create and initialize a fresh database in a unique temp directory.
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("tarragon-db-test-{}-{id}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("create the temp dir");
        let db = Database::open(dir.join("tarragon.db")).expect("open the temp database");
        db.initialize().expect("create the schema");
        Self { dir, db }
    }

    /// The database file path, for a second connection.
    fn path(&self) -> PathBuf {
        self.dir.join("tarragon.db")
    }
}

impl Drop for TempDb {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// Build a scan file under a folder.
fn file(folder: &str, name: &str, mtime: i64, size: u64) -> FileInfo {
    sized_file(folder, name, mtime, size, 0, 0)
}

/// Build a scan file under a folder with known pixel dimensions.
fn sized_file(
    folder: &str,
    name: &str,
    mtime: i64,
    size: u64,
    width: u32,
    height: u32,
) -> FileInfo {
    FileInfo {
        path: PathBuf::from(format!("{folder}/{name}")),
        size,
        mtime,
        width,
        height,
    }
}

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
fn scan_group_writes_every_row_and_commits_once() {
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
