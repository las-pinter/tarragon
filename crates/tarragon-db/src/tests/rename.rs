//! File rename tests.

use tarragon_core::entity::TagSource;

use crate::repository::{Thumbnail, ThumbnailWrite};

use super::support::TempDb;

/// Build a rendered row for `path` with all three cache links set.
fn row(path: &str) -> ThumbnailWrite {
    ThumbnailWrite {
        path: path.to_owned(),
        mtime: 10,
        size: 20,
        width: 30,
        height: 40,
        cache_uuid: "uuid".to_owned(),
        thumbnail_cache_path: Some(format!("{path}.thumb")),
        preview_cache_path: Some(format!("{path}.preview")),
        full_cache_path: Some(format!("{path}.full")),
        last_accessed: 5,
    }
}

/// Dump every row of the three path tables as text, in a fixed order.
fn snapshot(temp: &TempDb) -> Vec<String> {
    let conn = temp.db.lock().expect("lock the connection");
    let queries = [
        "SELECT path || '|' || mtime || '|' || size || '|' || ifnull(thumbnail_cache_path, 'null') \
         FROM thumbnails ORDER BY path",
        "SELECT path || '|' || tag_id FROM file_tags ORDER BY path, tag_id",
        "SELECT path || '|' || sort_order FROM favorites ORDER BY path",
    ];
    let mut out = Vec::new();
    for sql in queries {
        let mut stmt = conn.prepare(sql).expect("prepare");
        let rows = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .expect("query");
        out.extend(rows.map(|row| row.expect("row")));
    }
    out
}

/// Store a thumbnail, a user tag, and a favorite for `path`. Return the tag id.
fn seed(temp: &TempDb, path: &str, sort_order: i64) -> i64 {
    temp.db.upsert_thumbnail(&row(path)).expect("thumbnail");
    let tag = temp.db.ensure_tag("sky", TagSource::User).expect("tag");
    temp.db
        .add_tags_to_files_batch(&[path.to_owned()], std::slice::from_ref(&tag))
        .expect("tag the file");
    temp.db
        .add_favorite(path, Some("label"), sort_order)
        .expect("favorite");
    tag.id
}

/// Read a thumbnail row that must exist.
fn thumbnail(temp: &TempDb, path: &str) -> Thumbnail {
    temp.db
        .thumbnail(path)
        .expect("read")
        .expect("the thumbnail row exists")
}

#[test]
fn rename_file_moves_the_path_in_all_three_tables() {
    let temp = TempDb::new();
    let tag_id = seed(&temp, "/f/old.png", 7);
    temp.db
        .add_favorite("/f/other.png", None, 1)
        .expect("other");

    temp.db
        .rename_file("/f/old.png", "/f/new.png")
        .expect("rename");

    assert!(temp.db.thumbnail("/f/old.png").expect("read").is_none());
    assert!(temp.db.thumbnail("/f/new.png").expect("read").is_some());
    let tags = temp
        .db
        .tags_for_files(&["/f/old.png".to_owned(), "/f/new.png".to_owned()])
        .expect("tags");
    assert!(
        tags["/f/old.png"].is_empty(),
        "no tag stays on the old path"
    );
    let moved: Vec<i64> = tags["/f/new.png"].iter().map(|tag| tag.id).collect();
    assert_eq!(moved, [tag_id], "the tag moves with the file");
    let favorites = temp.db.list_favorites().expect("list");
    let rows: Vec<(&str, i64)> = favorites
        .iter()
        .map(|favorite| (favorite.path.as_str(), favorite.sort_order))
        .collect();
    assert_eq!(rows, [("/f/other.png", 1), ("/f/new.png", 7)]);
}

#[test]
fn rename_file_clears_cache_links_and_keeps_mtime_and_size() {
    let temp = TempDb::new();
    seed(&temp, "/f/old.png", 0);

    temp.db
        .rename_file("/f/old.png", "/f/new.png")
        .expect("rename");

    let moved = thumbnail(&temp, "/f/new.png");
    assert_eq!(moved.thumbnail_cache_path, None);
    assert_eq!(moved.preview_cache_path, None);
    assert_eq!(moved.full_cache_path, None);
    assert_eq!(moved.thumb_hash, None);
    assert_eq!((moved.mtime, moved.size), (10, 20));
    assert_eq!((moved.width, moved.height), (30, 40));
}

#[test]
fn rename_file_for_a_path_with_no_rows_is_ok_and_changes_nothing() {
    let temp = TempDb::new();
    seed(&temp, "/f/kept.png", 3);
    let before = snapshot(&temp);

    let result = temp.db.rename_file("/f/missing.png", "/f/new.png");

    assert!(result.is_ok(), "a missing path is not an error");
    assert_eq!(snapshot(&temp), before);
}

#[test]
fn rename_file_to_the_same_path_changes_nothing() {
    let temp = TempDb::new();
    seed(&temp, "/f/old.png", 7);
    let before = snapshot(&temp);

    temp.db
        .rename_file("/f/old.png", "/f/old.png")
        .expect("rename");

    assert_eq!(snapshot(&temp), before);
}

#[test]
fn rename_file_replaces_stale_tag_and_favorite_rows_at_the_new_path() {
    let temp = TempDb::new();
    let tag_id = seed(&temp, "/f/old.png", 7);
    let stale = temp.db.ensure_tag("stale", TagSource::User).expect("tag");
    temp.db
        .add_tags_to_files_batch(&["/f/new.png".to_owned()], &[stale])
        .expect("stale tag");
    temp.db
        .add_favorite("/f/new.png", Some("stale"), 2)
        .expect("stale favorite");

    temp.db
        .rename_file("/f/old.png", "/f/new.png")
        .expect("rename");

    let tags = temp
        .db
        .tags_for_files(&["/f/old.png".to_owned(), "/f/new.png".to_owned()])
        .expect("tags");
    assert!(
        tags["/f/old.png"].is_empty(),
        "no tag stays on the old path"
    );
    let kept: Vec<i64> = tags["/f/new.png"].iter().map(|tag| tag.id).collect();
    assert_eq!(kept, [tag_id], "only the renamed file's tag stays");
    let favorites = temp.db.list_favorites().expect("list");
    assert_eq!(favorites.len(), 1);
    assert_eq!(favorites[0].path, "/f/new.png");
    assert_eq!(favorites[0].label.as_deref(), Some("label"));
    assert_eq!(favorites[0].sort_order, 7);
}

#[test]
fn rename_file_replaces_a_stale_thumbnail_row_at_the_new_path() {
    let temp = TempDb::new();
    seed(&temp, "/f/old.png", 0);
    let stale = ThumbnailWrite {
        mtime: 99,
        size: 98,
        ..row("/f/new.png")
    };
    temp.db.upsert_thumbnail(&stale).expect("stale thumbnail");

    temp.db
        .rename_file("/f/old.png", "/f/new.png")
        .expect("rename");

    let moved = thumbnail(&temp, "/f/new.png");
    assert_eq!((moved.mtime, moved.size), (10, 20));
    assert_eq!(temp.db.count_thumbnails().expect("count"), 1);
}
