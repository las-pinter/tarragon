//! Tests for the job build from stored rows.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use tarragon_cache::store::SourceStamp;
use tarragon_db::repository::LibraryFile;

use super::library_jobs;
use crate::pipeline::{ErrorKind, Failures, ThumbError};

fn file(path: &str, cached: bool) -> LibraryFile {
    LibraryFile {
        path: path.to_owned(),
        mtime: 100,
        size: 7,
        cached,
    }
}

fn no_failures() -> Failures {
    Arc::new(Mutex::new(HashMap::new()))
}

#[test]
fn one_job_per_row_in_order_with_the_generation() {
    let files = [file("/a/x.png", true), file("/b/y.png", false)];
    let jobs = library_jobs(&files, 9, |_| "u".to_owned(), &no_failures());
    let paths: Vec<_> = jobs.iter().map(|job| job.path.clone()).collect();
    assert_eq!(
        paths,
        [PathBuf::from("/a/x.png"), PathBuf::from("/b/y.png")]
    );
    assert!(jobs.iter().all(|job| job.generation == 9));
    assert!(
        jobs.iter()
            .all(|job| job.stamp.mtime == 100 && job.stamp.size == 7)
    );
}

#[test]
fn uuid_of_runs_once_per_parent_folder() {
    let files = [
        file("/a/x.png", true),
        file("/b/y.png", true),
        file("/a/z.png", true),
    ];
    let mut calls = Vec::new();
    let jobs = library_jobs(
        &files,
        1,
        |parent| {
            calls.push(parent.to_owned());
            format!("id-{parent}")
        },
        &no_failures(),
    );
    assert_eq!(calls, ["/a", "/b"]);
    assert_eq!(jobs[2].uuid, "id-/a");
    assert_eq!(jobs[1].uuid, "id-/b");
}

#[test]
fn only_a_cached_row_has_a_stored_stamp() {
    let files = [file("/a/x.png", true), file("/a/y.png", false)];
    let jobs = library_jobs(&files, 1, |_| String::new(), &no_failures());
    assert_eq!(
        jobs[0].stored,
        Some(SourceStamp {
            mtime: 100,
            size: 7
        })
    );
    assert_eq!(jobs[1].stored, None);
}

#[test]
fn folder_is_the_parent_name_or_root() {
    let files = [file("/pics/art/x.png", true), file("/top.png", true)];
    let jobs = library_jobs(&files, 1, |_| String::new(), &no_failures());
    assert_eq!(jobs[0].folder, "art");
    assert_eq!(jobs[1].folder, "root");
}

#[test]
fn a_known_failure_needs_a_matching_stamp() {
    let failures = no_failures();
    let error = ThumbError {
        kind: ErrorKind::Decode,
        message: "bad".to_owned(),
    };
    let stamp = SourceStamp {
        mtime: 100,
        size: 7,
    };
    let old = SourceStamp { mtime: 99, size: 7 };
    {
        let mut map = failures.lock().expect("lock");
        map.insert(PathBuf::from("/a/same.png"), (stamp, error.clone()));
        map.insert(PathBuf::from("/a/changed.png"), (old, error.clone()));
    }
    let files = [file("/a/same.png", false), file("/a/changed.png", false)];
    let jobs = library_jobs(&files, 1, |_| String::new(), &failures);
    assert_eq!(jobs[0].known_failure, Some(error));
    assert_eq!(jobs[1].known_failure, None);
}

#[test]
fn edge_rows_get_a_zero_size_and_the_root_folder() {
    let mut negative = file("/a/neg.png", true);
    negative.size = -5;
    let files = [negative, file("/top.png", true), file("bare.png", true)];
    let mut calls = Vec::new();
    let jobs = library_jobs(
        &files,
        1,
        |parent| {
            calls.push(parent.to_owned());
            String::new()
        },
        &no_failures(),
    );
    assert_eq!(jobs[0].stamp.size, 0);
    assert_eq!(calls, ["/a", "/", ""]);
    assert_eq!(jobs[1].folder, "root");
    assert_eq!(jobs[2].folder, "root");
}
