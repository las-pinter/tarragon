//! Tests for `open_library`.

use std::time::Instant;

use super::*;
use crate::pipeline::PipelineError;

/// Open two folders with two files each and wait until every row is cached.
fn open_two_folders(setup: &mut Setup, dir_names: [&str; 2]) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    for name in dir_names {
        let folder = setup.dir.sub(name);
        for file in ["one.png", "two.png"] {
            write_png(&folder.join(file), 255);
            paths.push(folder.join(file));
        }
        setup.pipeline.open_folder(&folder).expect("open a folder");
        collect(&setup.results, 2);
    }
    let start = Instant::now();
    while setup
        .db
        .library_files()
        .expect("read the library")
        .iter()
        .any(|row| !row.cached)
    {
        assert!(start.elapsed() < WAIT, "the writer stores every row");
        std::thread::sleep(Duration::from_millis(10));
    }
    paths
}

#[test]
fn the_library_lists_the_files_of_both_folders() {
    let calls = Arc::default();
    let mut setup = start(TempDir::new(), 2, counting_render(&calls));
    let mut paths = open_two_folders(&mut setup, ["a", "b"]);
    let opened = setup.pipeline.open_library().expect("open the library");
    let mut files: Vec<PathBuf> = opened.files.iter().map(|file| file.path.clone()).collect();
    files.sort();
    paths.sort();
    assert_eq!(files, paths);
    assert!(
        opened
            .files
            .iter()
            .all(|file| file.width == 0 && file.height == 0 && file.size > 0)
    );
}

#[test]
fn a_cached_row_gives_its_thumbnail_without_a_render() {
    let calls = Arc::default();
    let mut setup = start(TempDir::new(), 2, counting_render(&calls));
    open_two_folders(&mut setup, ["a", "b"]);
    assert_eq!(calls.lock().expect("lock").len(), 4);

    setup.pipeline.open_library().expect("open the library");
    let results = collect(&setup.results, 4);
    assert!(results.iter().all(|result| result.outcome.is_ok()));
    assert_eq!(calls.lock().expect("lock").len(), 4, "the cache served all");
}

#[test]
fn the_library_open_starts_a_new_generation_and_drops_old_results() {
    let mut setup = start(
        TempDir::new(),
        1,
        Arc::new(tarragon_render::registry::render),
    );
    let folder = setup.dir.sub("a");
    for file in ["one.png", "two.png"] {
        write_png(&folder.join(file), 255);
    }
    let before = setup
        .pipeline
        .open_folder(&folder)
        .expect("open")
        .generation;
    // Wait for the results to sit in the channel without reading them.
    let start = Instant::now();
    while setup.repaints.load(Ordering::Relaxed) < 2 {
        assert!(start.elapsed() < WAIT, "the workers send both results");
        std::thread::sleep(Duration::from_millis(10));
    }

    let opened = setup.pipeline.open_library().expect("open the library");
    assert!(opened.generation > before);
    let results = collect(&setup.results, 2);
    assert!(
        results
            .iter()
            .all(|result| result.generation == opened.generation)
    );
    assert!(
        setup
            .results
            .recv_timeout(Duration::from_millis(200))
            .is_none(),
        "the results of the old generation are dropped"
    );
}

#[test]
fn an_empty_database_gives_no_files_and_no_job() {
    let mut setup = start(
        TempDir::new(),
        1,
        Arc::new(tarragon_render::registry::render),
    );
    let opened = setup.pipeline.open_library().expect("open the library");
    assert_eq!(opened.files.len(), 0);
    assert!(setup.pipeline.jobs.is_empty());
    assert!(
        setup
            .results
            .recv_timeout(Duration::from_millis(200))
            .is_none()
    );
}

#[test]
fn a_deleted_source_with_a_cached_row_still_gives_the_image() {
    let calls = Arc::default();
    let mut setup = start(TempDir::new(), 1, counting_render(&calls));
    let paths = open_two_folders(&mut setup, ["a", "b"]);
    for path in &paths {
        std::fs::remove_file(path).expect("delete a source");
    }

    let opened = setup.pipeline.open_library().expect("open the library");
    assert_eq!(opened.files.len(), 4);
    let results = collect(&setup.results, 4);
    assert!(results.iter().all(|result| result.outcome.is_ok()));
    assert_eq!(calls.lock().expect("lock").len(), 4, "no new render");
}

#[test]
fn a_folder_id_error_fails_the_open_and_keeps_the_old_state() {
    let mut setup = start(
        TempDir::new(),
        1,
        Arc::new(tarragon_render::registry::render),
    );
    let folder = setup.dir.sub("a");
    write_png(&folder.join("one.png"), 255);
    let before = setup
        .pipeline
        .open_folder(&folder)
        .expect("open")
        .generation;
    collect(&setup.results, 1);
    let jobs_before = setup.pipeline.jobs.len();
    // The rows stay readable, but the folder id table is gone.
    rusqlite::Connection::open(setup.dir.0.join("test.db"))
        .expect("open the db file")
        .execute_batch("DROP TABLE folder_cache_uuids;")
        .expect("drop the table");

    let result = setup.pipeline.open_library();
    assert!(matches!(result, Err(PipelineError::Db(_))));
    assert_eq!(setup.pipeline.generation.load(Ordering::Acquire), before);
    assert_eq!(setup.pipeline.jobs.len(), jobs_before);
    assert!(
        setup
            .pipeline
            .jobs
            .values()
            .all(|job| job.generation == before && !job.uuid.is_empty())
    );
}
