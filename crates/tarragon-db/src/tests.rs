//! Tests for the schema and the repositories.
//!
//! Every test uses a temporary database file. WAL needs a file, so the tests
//! do not use an in-memory database.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use tarragon_core::color::ColorBucket;
use tarragon_core::entity::{FileInfo, Tag, TagSource};
use tarragon_core::filter_state::{FilterScope, FilterState};

use crate::Database;
use crate::settings::{SettingKey, SettingValue, Settings, SettingsError, WINDOW_STATE_KEY};

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

/// A filter state with one folder chip.
fn scoped(folder: &str) -> FilterState {
    FilterState {
        folders: BTreeSet::from([PathBuf::from(folder)]),
        ..FilterState::default()
    }
}

/// Attach tags to one path.
fn attach_tags(temp: &TempDb, path: &str, tags: &[&Tag]) {
    let owned: Vec<Tag> = tags.iter().map(|tag| (*tag).clone()).collect();
    temp.db
        .add_tags_to_files_batch(&[path.to_owned()], &owned)
        .expect("attach the tags");
}

/// Populate the filter fixture: two folders, user tags, and color tags.
///
/// The rows match the Python `_populate_test_data` fixture, so the ported
/// behavior tests compare equal results.
fn populate_filter_fixture(temp: &TempDb) {
    let files = [
        file("/test/photos", "sunset_beach.png", 1, 100),
        file("/test/photos", "forest_path.jpg", 2, 200),
        file("/test/photos", "blue_ocean.jpg", 3, 300),
        file("/test/photos", "green_valley.png", 4, 150),
        file("/test/photos", "mountain_top.jpg", 5, 250),
        file("/test/photos", "doc.txt", 6, 10),
        file("/test/other", "beach.png", 7, 120),
    ];
    temp.db.write_scan_group(&files).expect("write the fixture");

    let beach = temp.db.ensure_tag("beach", TagSource::User).expect("tag");
    let vacation = temp
        .db
        .ensure_tag("vacation", TagSource::User)
        .expect("tag");
    let nature = temp.db.ensure_tag("nature", TagSource::User).expect("tag");
    let hiking = temp.db.ensure_tag("hiking", TagSource::User).expect("tag");
    let warm = temp
        .db
        .ensure_tag("warm", TagSource::AutoColor)
        .expect("tag");
    let green = temp
        .db
        .ensure_tag("green", TagSource::AutoColor)
        .expect("tag");
    let blue = temp
        .db
        .ensure_tag("blue", TagSource::AutoColor)
        .expect("tag");

    attach_tags(
        temp,
        "/test/photos/sunset_beach.png",
        &[&beach, &vacation, &warm],
    );
    attach_tags(
        temp,
        "/test/photos/forest_path.jpg",
        &[&nature, &hiking, &green],
    );
    attach_tags(
        temp,
        "/test/photos/blue_ocean.jpg",
        &[&beach, &vacation, &blue],
    );
    attach_tags(temp, "/test/photos/green_valley.png", &[&nature, &green]);
    attach_tags(temp, "/test/photos/mountain_top.jpg", &[&hiking, &blue]);
    attach_tags(temp, "/test/other/beach.png", &[&beach, &warm]);
}

/// Run the filter engine and return the stored paths.
fn filter_paths(temp: &TempDb, filters: &FilterState) -> Vec<String> {
    temp.db
        .filter_thumbnails(filters)
        .expect("run the filter query")
        .into_iter()
        .map(|row| row.path)
        .collect()
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

// ---------------------------------------------------------------------------
// Filter engine (task 13). Ported from the Python `test_query_service.py`.
// ---------------------------------------------------------------------------

#[test]
fn filter_engine_folder_scope_returns_every_file() {
    let temp = TempDb::new();
    populate_filter_fixture(&temp);
    assert_eq!(filter_paths(&temp, &scoped("/test/photos/")).len(), 6);
}

#[test]
fn filter_engine_global_scope_returns_every_file() {
    let temp = TempDb::new();
    populate_filter_fixture(&temp);
    assert_eq!(filter_paths(&temp, &FilterState::default()).len(), 7);
}

#[test]
fn filter_engine_returns_natural_full_path_order() {
    let temp = TempDb::new();
    for index in [1, 10, 2] {
        temp.db
            .write_scan_group(&[file(
                "/test/natural",
                &format!("photo {index}.png"),
                index,
                10,
            )])
            .expect("write the row");
    }
    assert_eq!(
        filter_paths(&temp, &scoped("/test/natural")),
        [
            "/test/natural/photo 1.png",
            "/test/natural/photo 2.png",
            "/test/natural/photo 10.png"
        ]
    );
}

#[test]
fn filter_engine_filename_filter_matches_a_substring() {
    let temp = TempDb::new();
    populate_filter_fixture(&temp);
    let filters = FilterState {
        text: "beach".to_owned(),
        ..scoped("/test/photos")
    };
    assert_eq!(
        filter_paths(&temp, &filters),
        ["/test/photos/sunset_beach.png"]
    );
}

#[test]
fn filter_engine_filename_filter_ignores_ascii_case() {
    let temp = TempDb::new();
    populate_filter_fixture(&temp);
    let filters = FilterState {
        text: "BEACH".to_owned(),
        ..scoped("/test/photos")
    };
    assert_eq!(
        filter_paths(&temp, &filters),
        ["/test/photos/sunset_beach.png"]
    );
}

#[test]
fn filter_engine_filename_filter_escapes_like_wildcards() {
    let temp = TempDb::new();
    populate_filter_fixture(&temp);
    for text in ["%_", "do_"] {
        let filters = FilterState {
            text: text.to_owned(),
            ..scoped("/test/photos")
        };
        assert_eq!(
            filter_paths(&temp, &filters),
            Vec::<String>::new(),
            "the text {text} matched as a LIKE wildcard"
        );
    }
}

#[test]
fn filter_engine_filename_filter_escapes_a_literal_backslash() {
    let temp = TempDb::new();
    populate_filter_fixture(&temp);
    // Without the escape do\c would match doc.txt; Q51 records this delta in the study decisions record.
    let filters = FilterState {
        text: "do\\c".to_owned(),
        ..scoped("/test/photos")
    };
    assert_eq!(filter_paths(&temp, &filters), Vec::<String>::new());
}

#[test]
fn filter_engine_filename_filter_does_not_trim_space() {
    let temp = TempDb::new();
    populate_filter_fixture(&temp);
    // The leading space stays, so this must not match sunset_beach.png.
    let filters = FilterState {
        text: " beach".to_owned(),
        ..scoped("/test/photos")
    };
    assert_eq!(filter_paths(&temp, &filters), Vec::<String>::new());
}

#[test]
fn filter_engine_whitespace_only_text_is_one_active_filter() {
    let temp = TempDb::new();
    temp.db
        .write_scan_group(&[
            file("/ws", "with space.png", 1, 10),
            file("/ws", "plain.png", 2, 10),
        ])
        .expect("write the rows");
    let filters = FilterState {
        text: " ".to_owned(),
        ..FilterState::default()
    };
    assert_eq!(filters.active_count(FilterScope::Global), 1);
    assert_eq!(filter_paths(&temp, &filters), ["/ws/with space.png"]);
}

#[test]
fn filter_engine_tags_require_all_selected_tags() {
    let temp = TempDb::new();
    populate_filter_fixture(&temp);
    let filters = FilterState {
        tags: BTreeSet::from(["beach".to_owned(), "vacation".to_owned()]),
        ..scoped("/test/photos")
    };
    assert_eq!(
        filter_paths(&temp, &filters),
        [
            "/test/photos/blue_ocean.jpg",
            "/test/photos/sunset_beach.png"
        ]
    );
    // nature OR hiking also returns green_valley and mountain_top, so this
    // pair proves the AND.
    let separate = FilterState {
        tags: BTreeSet::from(["nature".to_owned(), "hiking".to_owned()]),
        ..scoped("/test/photos")
    };
    assert_eq!(
        filter_paths(&temp, &separate),
        ["/test/photos/forest_path.jpg"]
    );
}

#[test]
fn filter_engine_tags_with_no_common_file_return_empty() {
    let temp = TempDb::new();
    populate_filter_fixture(&temp);
    let filters = FilterState {
        tags: BTreeSet::from(["beach".to_owned(), "nature".to_owned()]),
        ..scoped("/test/photos")
    };
    assert_eq!(filter_paths(&temp, &filters), Vec::<String>::new());
}

#[test]
fn filter_engine_color_filter_matches_one_bucket() {
    let temp = TempDb::new();
    populate_filter_fixture(&temp);
    let filters = FilterState {
        colors: BTreeSet::from([ColorBucket::Green]),
        ..scoped("/test/photos")
    };
    assert_eq!(
        filter_paths(&temp, &filters),
        [
            "/test/photos/forest_path.jpg",
            "/test/photos/green_valley.png"
        ]
    );
}

#[test]
fn filter_engine_colors_require_all_selected_buckets() {
    let temp = TempDb::new();
    populate_filter_fixture(&temp);
    let filters = FilterState {
        colors: BTreeSet::from([ColorBucket::Green, ColorBucket::Blue]),
        ..scoped("/test/photos")
    };
    assert_eq!(filter_paths(&temp, &filters), Vec::<String>::new());
}

#[test]
fn filter_engine_tag_and_color_filters_combine_with_and() {
    let temp = TempDb::new();
    populate_filter_fixture(&temp);
    // Every nature row also has green, so OR and AND agree here.
    let filters = FilterState {
        tags: BTreeSet::from(["nature".to_owned()]),
        colors: BTreeSet::from([ColorBucket::Green]),
        ..scoped("/test/photos")
    };
    assert_eq!(
        filter_paths(&temp, &filters),
        [
            "/test/photos/forest_path.jpg",
            "/test/photos/green_valley.png"
        ]
    );
    // hiking OR green also returns mountain_top and green_valley, so this
    // pair proves the AND.
    let separate = FilterState {
        tags: BTreeSet::from(["hiking".to_owned()]),
        colors: BTreeSet::from([ColorBucket::Green]),
        ..scoped("/test/photos")
    };
    assert_eq!(
        filter_paths(&temp, &separate),
        ["/test/photos/forest_path.jpg"]
    );
}

#[test]
fn filter_engine_folders_combine_with_or() {
    let temp = TempDb::new();
    populate_filter_fixture(&temp);
    let filters = FilterState {
        folders: BTreeSet::from([PathBuf::from("/test/photos"), PathBuf::from("/test/other")]),
        ..FilterState::default()
    };
    assert_eq!(filter_paths(&temp, &filters).len(), 7);
}

#[test]
fn filter_engine_unknown_folder_returns_empty() {
    let temp = TempDb::new();
    populate_filter_fixture(&temp);
    assert_eq!(
        filter_paths(&temp, &scoped("/nonexistent")),
        Vec::<String>::new()
    );
}

#[test]
fn filter_engine_folder_does_not_match_a_sibling_prefix() {
    let temp = TempDb::new();
    temp.db
        .write_scan_group(&[
            file("/photos", "img.png", 1, 10),
            file("/photos-vacation", "img.png", 2, 20),
        ])
        .expect("write the rows");
    assert_eq!(filter_paths(&temp, &scoped("/photos")), ["/photos/img.png"]);
}

#[test]
fn filter_engine_folder_escapes_an_underscore() {
    let temp = TempDb::new();
    temp.db
        .write_scan_group(&[
            file("/my_photos", "a.png", 1, 10),
            file("/myXphotos", "b.png", 2, 10),
        ])
        .expect("write the rows");
    assert_eq!(
        filter_paths(&temp, &scoped("/my_photos")),
        ["/my_photos/a.png"]
    );
}

#[test]
fn filter_engine_folder_escapes_a_percent() {
    let temp = TempDb::new();
    temp.db
        .write_scan_group(&[
            file("/my%photos", "a.png", 1, 10),
            file("/myXXphotos", "b.png", 2, 10),
        ])
        .expect("write the rows");
    assert_eq!(
        filter_paths(&temp, &scoped("/my%photos")),
        ["/my%photos/a.png"]
    );
}

#[test]
fn filter_engine_folder_normalizes_backslash_separators() {
    let temp = TempDb::new();
    temp.db
        .write_scan_group(&[
            file("D:/Dropbox/Art/Speedpaintings", "img1.jpg", 1, 10),
            file("D:/Dropbox/Art/Other", "img2.jpg", 2, 10),
        ])
        .expect("write the rows");
    let filters = scoped("D:\\Dropbox\\Art\\Speedpaintings");
    assert_eq!(
        filter_paths(&temp, &filters),
        ["D:/Dropbox/Art/Speedpaintings/img1.jpg"]
    );
}

#[test]
fn filter_engine_has_ten_color_buckets() {
    assert_eq!(ColorBucket::ALL.len(), 10);
    let temp = TempDb::new();
    for bucket in ColorBucket::ALL {
        let name = bucket.as_str();
        temp.db
            .write_scan_group(&[file("/buckets", &format!("{name}.png"), 1, 10)])
            .expect("write the row");
        let tag = temp
            .db
            .ensure_tag(name, TagSource::AutoColor)
            .expect("make the bucket tag");
        temp.db
            .add_tags_to_files_batch(&[format!("/buckets/{name}.png")], &[tag])
            .expect("attach the bucket tag");
    }
    for bucket in ColorBucket::ALL {
        let filters = FilterState {
            colors: BTreeSet::from([bucket]),
            ..FilterState::default()
        };
        assert_eq!(
            filter_paths(&temp, &filters),
            [format!("/buckets/{}.png", bucket.as_str())]
        );
    }
}

// ---------------------------------------------------------------------------
// settings registry
// ---------------------------------------------------------------------------

/// A valid value for every key, used by the round-trip and default tests.
fn settings_sample_value(key: SettingKey) -> SettingValue {
    match key {
        SettingKey::CacheDir => SettingValue::Text("/tmp/tarragon-cache".to_owned()),
        SettingKey::CacheFormat => SettingValue::Text("jpeg".to_owned()),
        SettingKey::CacheSizeCapGb => SettingValue::Int(50),
        SettingKey::ColorTagEnabled | SettingKey::ClearFullResOnExit => SettingValue::Bool(false),
        SettingKey::ColorTagPaletteSize => SettingValue::Int(16),
        SettingKey::ColorTagMinShare => SettingValue::Float(0.25),
        SettingKey::ColorTagNeutralSThreshold => SettingValue::Float(0.30),
        SettingKey::DebugMode => SettingValue::Bool(true),
        SettingKey::LargeCanvasThresholdMp => SettingValue::Float(50.5),
        SettingKey::MaxMultiPreview => SettingValue::Int(25),
        SettingKey::MaxPsdWorkers => SettingValue::Int(5),
        SettingKey::TileGridSize => SettingValue::Text("2x2".to_owned()),
    }
}

#[test]
fn settings_have_thirteen_unique_typed_keys() {
    assert_eq!(SettingKey::ALL.len(), 13);
    let names: BTreeSet<&str> = SettingKey::ALL.iter().map(|key| key.as_str()).collect();
    assert_eq!(names.len(), SettingKey::ALL.len());
}

#[test]
fn settings_load_every_default_on_a_fresh_database() {
    let temp = TempDb::new();
    let settings = Settings::new(&temp.db);
    for key in SettingKey::ALL {
        assert_eq!(
            settings.get(key).expect("read the default"),
            key.default_value(),
            "wrong default for {}",
            key.as_str()
        );
    }
}

#[test]
fn settings_round_trip_a_set_value_for_every_key() {
    let temp = TempDb::new();
    let settings = Settings::new(&temp.db);
    for key in SettingKey::ALL {
        let value = settings_sample_value(key);
        settings.set(key, value.clone()).expect("set the value");
        assert_eq!(
            settings.get(key).expect("read the value"),
            value,
            "no round trip for {}",
            key.as_str()
        );
    }
}

#[test]
fn cache_format_defaults_to_jpeg() {
    let temp = TempDb::new();
    let settings = Settings::new(&temp.db);
    assert_eq!(
        settings.get(SettingKey::CacheFormat).expect("read"),
        SettingValue::Text("jpeg".to_owned())
    );
}

#[test]
fn cache_size_cap_gb_defaults_to_twenty() {
    let temp = TempDb::new();
    let settings = Settings::new(&temp.db);
    assert_eq!(
        settings.get(SettingKey::CacheSizeCapGb).expect("read"),
        SettingValue::Int(20)
    );
}

#[test]
fn cache_format_rejects_an_unknown_choice() {
    let temp = TempDb::new();
    let settings = Settings::new(&temp.db);
    let error = settings
        .set(
            SettingKey::CacheFormat,
            SettingValue::Text("png".to_owned()),
        )
        .expect_err("png is not a valid choice");
    assert!(
        matches!(error, SettingsError::InvalidChoice { .. }),
        "{error}"
    );
    assert_eq!(temp.db.setting("cache_format").expect("read raw"), None);
}

#[test]
fn cache_format_stored_unknown_choice_falls_back_to_jpeg() {
    let temp = TempDb::new();
    temp.db
        .set_setting("cache_format", "\"PNG\"")
        .expect("seed the bad value");
    let settings = Settings::new(&temp.db);
    assert_eq!(
        settings.get(SettingKey::CacheFormat).expect("read"),
        SettingValue::Text("jpeg".to_owned())
    );
}

#[test]
fn settings_clamp_numeric_values_above_the_maximum() {
    let temp = TempDb::new();
    let settings = Settings::new(&temp.db);
    settings
        .set(SettingKey::ColorTagPaletteSize, SettingValue::Int(100))
        .expect("set");
    assert_eq!(
        settings.get(SettingKey::ColorTagPaletteSize).expect("read"),
        SettingValue::Int(32)
    );
}

#[test]
fn settings_clamp_numeric_values_below_the_minimum() {
    let temp = TempDb::new();
    let settings = Settings::new(&temp.db);
    settings
        .set(SettingKey::ColorTagPaletteSize, SettingValue::Int(0))
        .expect("set");
    assert_eq!(
        settings.get(SettingKey::ColorTagPaletteSize).expect("read"),
        SettingValue::Int(2)
    );
}

#[test]
fn settings_clamp_a_stored_float_above_the_maximum() {
    let temp = TempDb::new();
    let settings = Settings::new(&temp.db);
    settings
        .set(
            SettingKey::LargeCanvasThresholdMp,
            SettingValue::Float(9999.0),
        )
        .expect("set");
    assert_eq!(
        settings
            .get(SettingKey::LargeCanvasThresholdMp)
            .expect("read"),
        SettingValue::Float(1000.0)
    );
}

#[test]
fn cache_size_cap_gb_clamps_to_ten_thousand() {
    let temp = TempDb::new();
    let settings = Settings::new(&temp.db);
    settings
        .set(SettingKey::CacheSizeCapGb, SettingValue::Int(20_000))
        .expect("set");
    assert_eq!(
        settings.get(SettingKey::CacheSizeCapGb).expect("read"),
        SettingValue::Int(10_000)
    );
}

#[test]
fn cache_size_cap_gb_zero_disables_the_cap() {
    let temp = TempDb::new();
    let settings = Settings::new(&temp.db);
    settings
        .set(SettingKey::CacheSizeCapGb, SettingValue::Int(0))
        .expect("set");
    assert_eq!(
        settings.get(SettingKey::CacheSizeCapGb).expect("read"),
        SettingValue::Int(0)
    );
}

#[test]
fn settings_clamp_a_stored_value_out_of_range_on_read() {
    let temp = TempDb::new();
    temp.db
        .set_setting("color_tag_palette_size", "99")
        .expect("seed the bad value");
    let settings = Settings::new(&temp.db);
    assert_eq!(
        settings.get(SettingKey::ColorTagPaletteSize).expect("read"),
        SettingValue::Int(32)
    );
}

#[test]
fn settings_repair_a_stored_bool_for_an_int_key() {
    let temp = TempDb::new();
    temp.db
        .set_setting("max_psd_workers", "true")
        .expect("seed the bad value");
    let settings = Settings::new(&temp.db);
    assert_eq!(
        settings.get(SettingKey::MaxPsdWorkers).expect("read"),
        SettingValue::Int(3)
    );
}

#[test]
fn settings_repair_a_stored_float_for_an_int_key() {
    let temp = TempDb::new();
    temp.db
        .set_setting("max_psd_workers", "7.5")
        .expect("seed the bad value");
    let settings = Settings::new(&temp.db);
    assert_eq!(
        settings.get(SettingKey::MaxPsdWorkers).expect("read"),
        SettingValue::Int(3)
    );
}

#[test]
fn settings_repair_a_stored_int_for_a_float_key() {
    let temp = TempDb::new();
    temp.db
        .set_setting("large_canvas_threshold_mp", "50")
        .expect("seed the bad value");
    let settings = Settings::new(&temp.db);
    assert_eq!(
        settings
            .get(SettingKey::LargeCanvasThresholdMp)
            .expect("read"),
        SettingValue::Float(20.0)
    );
}

#[test]
fn settings_repair_malformed_stored_json_to_the_default() {
    let temp = TempDb::new();
    temp.db
        .set_setting("debug_mode", "{not-json")
        .expect("seed the bad value");
    let settings = Settings::new(&temp.db);
    assert_eq!(
        settings.get(SettingKey::DebugMode).expect("read"),
        SettingValue::Bool(false)
    );
}

#[test]
fn set_rejects_a_float_for_an_int_setting_and_does_not_persist() {
    let temp = TempDb::new();
    let settings = Settings::new(&temp.db);
    let error = settings
        .set(SettingKey::MaxPsdWorkers, SettingValue::Float(4.5))
        .expect_err("a float is not an int");
    assert!(
        matches!(error, SettingsError::TypeMismatch { .. }),
        "{error}"
    );
    assert_eq!(temp.db.setting("max_psd_workers").expect("read raw"), None);
}

#[test]
fn set_rejects_an_int_for_a_bool_setting_and_does_not_persist() {
    let temp = TempDb::new();
    let settings = Settings::new(&temp.db);
    let error = settings
        .set(SettingKey::DebugMode, SettingValue::Int(1))
        .expect_err("an int is not a bool");
    assert!(
        matches!(error, SettingsError::TypeMismatch { .. }),
        "{error}"
    );
    assert_eq!(temp.db.setting("debug_mode").expect("read raw"), None);
}

#[test]
fn set_rejects_a_string_for_an_int_setting_and_does_not_persist() {
    let temp = TempDb::new();
    let settings = Settings::new(&temp.db);
    let error = settings
        .set(
            SettingKey::MaxPsdWorkers,
            SettingValue::Text("abc".to_owned()),
        )
        .expect_err("a string is not an int");
    assert!(
        matches!(error, SettingsError::TypeMismatch { .. }),
        "{error}"
    );
    assert_eq!(temp.db.setting("max_psd_workers").expect("read raw"), None);
}

#[test]
fn set_persists_the_value_as_json_text() {
    let temp = TempDb::new();
    let settings = Settings::new(&temp.db);
    settings
        .set(SettingKey::MaxPsdWorkers, SettingValue::Int(5))
        .expect("set the int");
    settings
        .set(
            SettingKey::CacheFormat,
            SettingValue::Text("jpeg".to_owned()),
        )
        .expect("set the string");
    assert_eq!(
        temp.db.setting("max_psd_workers").expect("read raw"),
        Some("5".to_owned())
    );
    assert_eq!(
        temp.db.setting("cache_format").expect("read raw"),
        Some("\"jpeg\"".to_owned())
    );
}

#[test]
fn tile_grid_accepts_one_to_four() {
    let temp = TempDb::new();
    let settings = Settings::new(&temp.db);
    for size in ["1x1", "2x2", "3x3", "4x4"] {
        settings
            .set(
                SettingKey::TileGridSize,
                SettingValue::Text(size.to_owned()),
            )
            .expect("set the grid size");
        assert_eq!(
            settings.get(SettingKey::TileGridSize).expect("read"),
            SettingValue::Text(size.to_owned())
        );
    }
}

#[test]
fn tile_grid_rejects_five_by_five_and_does_not_persist() {
    let temp = TempDb::new();
    let settings = Settings::new(&temp.db);
    let error = settings
        .set(
            SettingKey::TileGridSize,
            SettingValue::Text("5x5".to_owned()),
        )
        .expect_err("5x5 is out of range");
    assert!(
        matches!(error, SettingsError::InvalidChoice { .. }),
        "{error}"
    );
    assert_eq!(temp.db.setting("tile_grid_size").expect("read raw"), None);
}

#[test]
fn tile_grid_rejects_an_asymmetric_grid() {
    let temp = TempDb::new();
    let settings = Settings::new(&temp.db);
    let error = settings
        .set(
            SettingKey::TileGridSize,
            SettingValue::Text("4x2".to_owned()),
        )
        .expect_err("4x2 is not square");
    assert!(
        matches!(error, SettingsError::InvalidChoice { .. }),
        "{error}"
    );
}

#[test]
fn tile_grid_repairs_a_stored_out_of_range_grid() {
    let temp = TempDb::new();
    temp.db
        .set_setting("tile_grid_size", "\"99x99\"")
        .expect("seed the bad value");
    let settings = Settings::new(&temp.db);
    assert_eq!(
        settings.get(SettingKey::TileGridSize).expect("read"),
        SettingValue::Text("3x3".to_owned())
    );
}

#[test]
fn settings_accept_the_max_psd_workers_bounds() {
    let temp = TempDb::new();
    let settings = Settings::new(&temp.db);
    for workers in [1, 8] {
        settings
            .set(SettingKey::MaxPsdWorkers, SettingValue::Int(workers))
            .expect("set the workers");
        assert_eq!(
            settings.get(SettingKey::MaxPsdWorkers).expect("read"),
            SettingValue::Int(workers)
        );
    }
}

#[test]
fn settings_accept_the_max_multi_preview_bounds() {
    let temp = TempDb::new();
    let settings = Settings::new(&temp.db);
    for previews in [1, 100] {
        settings
            .set(SettingKey::MaxMultiPreview, SettingValue::Int(previews))
            .expect("set the previews");
        assert_eq!(
            settings.get(SettingKey::MaxMultiPreview).expect("read"),
            SettingValue::Int(previews)
        );
    }
}

#[test]
fn settings_accept_the_cache_size_cap_gb_maximum() {
    let temp = TempDb::new();
    let settings = Settings::new(&temp.db);
    settings
        .set(SettingKey::CacheSizeCapGb, SettingValue::Int(10_000))
        .expect("set the cap");
    assert_eq!(
        settings.get(SettingKey::CacheSizeCapGb).expect("read"),
        SettingValue::Int(10_000)
    );
}

#[test]
fn cache_format_rejects_an_uppercase_choice() {
    let temp = TempDb::new();
    let settings = Settings::new(&temp.db);
    let error = settings
        .set(
            SettingKey::CacheFormat,
            SettingValue::Text("JPEG".to_owned()),
        )
        .expect_err("JPEG is not a valid choice");
    assert!(
        matches!(error, SettingsError::InvalidChoice { .. }),
        "{error}"
    );
    assert_eq!(temp.db.setting("cache_format").expect("read raw"), None);
}

#[test]
fn set_rejects_a_non_finite_float_and_does_not_persist() {
    let temp = TempDb::new();
    let settings = Settings::new(&temp.db);
    for value in [f64::INFINITY, f64::NAN] {
        let error = settings
            .set(
                SettingKey::LargeCanvasThresholdMp,
                SettingValue::Float(value),
            )
            .expect_err("a non-finite float is not valid");
        assert!(matches!(error, SettingsError::NonFinite { .. }), "{error}");
    }
    assert_eq!(
        temp.db
            .setting("large_canvas_threshold_mp")
            .expect("read raw"),
        None
    );
}

#[test]
fn window_state_defaults_to_unset() {
    let temp = TempDb::new();
    let settings = Settings::new(&temp.db);
    assert_eq!(settings.window_state().expect("read"), None);
}

#[test]
fn window_state_round_trips_json_text() {
    let temp = TempDb::new();
    let settings = Settings::new(&temp.db);
    let state = r#"{"layout":"library","geometry":[0,0,1200,800],"sort_mode":"date"}"#;
    settings.set_window_state(state).expect("write the state");
    assert_eq!(
        settings.window_state().expect("read").as_deref(),
        Some(state)
    );
}

#[test]
fn window_state_rejects_malformed_json_and_does_not_persist() {
    let temp = TempDb::new();
    let settings = Settings::new(&temp.db);
    let error = settings
        .set_window_state("{not-json")
        .expect_err("malformed JSON is refused");
    assert!(
        matches!(error, SettingsError::InvalidWindowState { .. }),
        "{error}"
    );
    assert_eq!(settings.window_state().expect("read"), None);
}

#[test]
fn window_state_is_not_a_typed_setting() {
    let names: BTreeSet<&str> = SettingKey::ALL.iter().map(|key| key.as_str()).collect();
    assert!(!names.contains(WINDOW_STATE_KEY));
}
