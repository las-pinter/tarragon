use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use rusqlite::trace::{TraceEvent, TraceEventCodes};

use crate::repository::{ThumbnailWrite, WriteBatch};

use super::support::TempDb;

/// Build a rendered row for `path`.
fn row(path: &str) -> ThumbnailWrite {
    ThumbnailWrite {
        path: path.to_owned(),
        mtime: 10,
        size: 20,
        width: 30,
        height: 40,
        cache_uuid: "uuid".to_owned(),
        thumbnail_cache_path: Some(format!("{path}.thumb")),
        preview_cache_path: None,
        full_cache_path: None,
        last_accessed: 0,
    }
}

/// Count the commits on the shared connection from now on.
fn count_commits(temp: &TempDb) -> Arc<AtomicUsize> {
    let commits = Arc::new(AtomicUsize::new(0));
    let seen = Arc::clone(&commits);
    temp.db
        .lock()
        .expect("lock the connection")
        .commit_hook(Some(move || {
            seen.fetch_add(1, Ordering::SeqCst);
            false
        }))
        .expect("set the commit hook");
    commits
}

/// Build the paths `/pics/0` up to `/pics/{count - 1}`.
fn paths(count: usize) -> Vec<String> {
    (0..count).map(|index| format!("/pics/{index}")).collect()
}

/// A mixed batch commits once and stores every item.
#[test]
fn a_mixed_batch_commits_once_and_stores_every_item() {
    let temp = TempDb::new();
    let all = paths(10);
    let commits = count_commits(&temp);
    let batch = WriteBatch {
        rows: all.iter().map(|path| row(path)).collect(),
        touched: all[..5].to_vec(),
        touched_at: 777,
        colors: all[..4]
            .iter()
            .map(|path| (path.clone(), vec!["red".to_owned(), "blue".to_owned()]))
            .collect(),
    };

    let report = temp.db.apply_writes(&batch).expect("apply the batch");

    assert_eq!(commits.load(Ordering::SeqCst), 1);
    assert!(report.stored);
    assert_eq!(report.failed, 0);
    assert_eq!(temp.db.count_thumbnails().expect("count"), 10);
    for (index, path) in all.iter().enumerate() {
        let stored = temp.db.thumbnail(path).expect("read").expect("row exists");
        let expected = if index < 5 { 777 } else { 0 };
        assert_eq!(stored.last_accessed, expected, "{path}");
    }
    let tags = temp.db.tags_for_files(&all).expect("read the tags");
    for (index, path) in all.iter().enumerate() {
        let names: Vec<&str> = tags
            .get(path)
            .map(|list| list.iter().map(|tag| tag.name.as_str()).collect())
            .unwrap_or_default();
        if index < 4 {
            assert_eq!(names.len(), 2, "{path}");
            assert!(names.contains(&"red") && names.contains(&"blue"), "{path}");
        } else {
            assert!(names.is_empty(), "{path}");
        }
    }
}

/// An empty batch opens no transaction and stores nothing.
#[test]
fn an_empty_batch_opens_no_transaction() {
    let temp = TempDb::new();
    let begins = Arc::new(AtomicUsize::new(0));
    let seen = Arc::clone(&begins);
    temp.db
        .lock()
        .expect("lock the connection")
        .trace_v2(
            TraceEventCodes::SQLITE_TRACE_STMT,
            Some(move |event: TraceEvent<'_>| {
                if let TraceEvent::Stmt(_, sql) = event
                    && sql.trim_start().to_ascii_uppercase().starts_with("BEGIN")
                {
                    seen.fetch_add(1, Ordering::SeqCst);
                }
            }),
        )
        .expect("set the trace");

    let report = temp
        .db
        .apply_writes(&WriteBatch::default())
        .expect("apply the empty batch");

    assert_eq!(begins.load(Ordering::SeqCst), 0);
    assert!(!report.stored);
    assert_eq!(report.failed, 0);
}

/// A failing item rolls back alone. The good items commit once.
#[test]
fn a_failing_item_does_not_lose_the_others() {
    let temp = TempDb::new();
    temp.db
        .lock()
        .expect("lock the connection")
        .execute_batch(
            "CREATE TRIGGER t BEFORE INSERT ON thumbnails WHEN NEW.path = '/bad' \
             BEGIN SELECT RAISE(ABORT, 'x'); END;",
        )
        .expect("create the trigger");
    let commits = count_commits(&temp);
    let batch = WriteBatch {
        rows: vec![row("/a"), row("/b"), row("/bad"), row("/c")],
        ..WriteBatch::default()
    };

    let report = temp.db.apply_writes(&batch).expect("apply the batch");

    assert_eq!(report.failed, 1);
    assert!(report.stored);
    assert_eq!(commits.load(Ordering::SeqCst), 1);
    assert_eq!(temp.db.count_thumbnails().expect("count"), 3);
    assert!(temp.db.thumbnail("/bad").expect("read").is_none());
}

/// A colour write that fails half way leaves no partial tags behind.
#[test]
fn a_failed_colour_write_rolls_back_its_own_changes() {
    let temp = TempDb::new();
    temp.db
        .lock()
        .expect("lock the connection")
        .execute_batch(
            "CREATE TRIGGER c BEFORE INSERT ON file_tags WHEN NEW.path = '/bad' \
             AND NEW.tag_id = (SELECT id FROM tags WHERE name = 'blue') \
             BEGIN SELECT RAISE(ABORT, 'x'); END;",
        )
        .expect("create the trigger");
    let batch = WriteBatch {
        colors: vec![
            ("/bad".to_owned(), vec!["red".to_owned(), "blue".to_owned()]),
            ("/good".to_owned(), vec!["red".to_owned()]),
        ],
        ..WriteBatch::default()
    };

    let report = temp.db.apply_writes(&batch).expect("apply the batch");

    assert_eq!(report.failed, 1);
    assert!(report.stored);
    let tags = temp
        .db
        .tags_for_files(&["/bad".to_owned(), "/good".to_owned()])
        .expect("read the tags");
    assert!(tags.get("/bad").is_none_or(Vec::is_empty));
    assert_eq!(tags.get("/good").map(Vec::len), Some(1));
}

/// A trigger that ends the whole transaction stops the call. Later items do
/// not run in autocommit.
#[test]
fn an_ended_transaction_stops_the_batch() {
    let temp = TempDb::new();
    temp.db
        .lock()
        .expect("lock the connection")
        .execute_batch(
            "CREATE TRIGGER r BEFORE INSERT ON thumbnails WHEN NEW.path = '/bad' \
             BEGIN SELECT RAISE(ROLLBACK, 'x'); END;",
        )
        .expect("create the trigger");
    let batch = WriteBatch {
        rows: vec![row("/a"), row("/bad"), row("/c")],
        ..WriteBatch::default()
    };

    let result = temp.db.apply_writes(&batch);

    assert!(result.is_err());
    assert_eq!(temp.db.count_thumbnails().expect("count"), 0);
}
