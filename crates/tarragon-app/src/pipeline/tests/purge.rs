//! Tests for `Pipeline::purge`.

use tarragon_core::entity::TagSource;

use super::drain::{DRAIN_WAIT, STILL_WAITING, open_held};
use super::*;
use crate::pipeline::PipelineError;

/// The regular files under `dir`, at any depth.
fn files_under(dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return found;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            found.extend(files_under(&path));
        } else {
            found.push(path);
        }
    }
    found
}

/// A data directory next to the cache, as the app would pass it.
fn data_dir(setup: &Setup) -> PathBuf {
    setup.dir.sub("data")
}

/// Open a folder of `names` and wait until its rows are stored.
fn open_stored(setup: &mut Setup, names: &[&str]) -> PathBuf {
    let folder = setup.dir.sub("pics");
    for name in names {
        write_png(&folder.join(name), 255);
    }
    setup.pipeline.open_folder(&folder).expect("open");
    collect(&setup.results, names.len());
    setup.pipeline.flush();
    folder
}

#[test]
fn a_purge_removes_the_files_and_rows_and_keeps_the_cache_root() {
    let mut setup = start(TempDir::new(), 2, counting_render(&Arc::default()));
    open_stored(&mut setup, &["a.png", "b.png"]);
    assert!(files_under(&setup.cache_root).len() > 1);
    assert_eq!(setup.db.count_thumbnails().expect("count"), 2);

    let outcome = setup
        .pipeline
        .purge(&data_dir(&setup))
        .expect("the purge works");

    assert!(outcome.files_removed > 0);
    assert_eq!(files_under(&setup.cache_root).len(), 0);
    assert!(setup.cache_root.is_dir());
    assert_eq!(setup.db.count_thumbnails().expect("count"), 0);
}

#[test]
fn a_purge_keeps_the_file_tags() {
    let mut setup = start(TempDir::new(), 2, counting_render(&Arc::default()));
    let folder = open_stored(&mut setup, &["a.png"]);
    let path = path_text(&folder.join("a.png"));
    let tag = setup
        .db
        .ensure_tag("study", TagSource::User)
        .expect("make the tag");
    setup
        .db
        .add_tags_to_files_batch(std::slice::from_ref(&path), &[tag])
        .expect("tag the file");

    setup
        .pipeline
        .purge(&data_dir(&setup))
        .expect("the purge works");

    let tags = setup
        .db
        .tags_for_files(std::slice::from_ref(&path))
        .expect("read the tags");
    assert!(tags[&path].iter().any(|tag| tag.name == "study"));
}

#[test]
fn an_unsafe_root_is_refused_and_the_rows_stay() {
    let mut setup = start(TempDir::new(), 2, counting_render(&Arc::default()));
    open_stored(&mut setup, &["a.png"]);
    let files = files_under(&setup.cache_root).len();
    // The cache root is the data directory, then the parent of it.
    let inside = setup.cache_root.join("data");
    std::fs::create_dir_all(&inside).expect("make the data dir");

    for protected in [setup.cache_root.clone(), inside] {
        let error = setup.pipeline.purge(&protected).expect_err("refused");
        assert!(matches!(
            error,
            PipelineError::Cache(tarragon_cache::CacheError::UnsafeRoot(_))
        ));
        assert_eq!(setup.db.count_thumbnails().expect("count"), 1);
        assert_eq!(files_under(&setup.cache_root).len(), files);
    }
}

#[test]
fn a_purge_waits_for_a_running_render_and_leaves_none_of_its_files() {
    let (mut setup, gate) = open_held(1, false);
    let data = data_dir(&setup);
    let queue = Arc::clone(&setup.pipeline.queue);
    let (done_tx, done_rx) = crossbeam_channel::bounded::<()>(1);
    std::thread::scope(|scope| {
        scope.spawn(|| {
            let result = setup.pipeline.purge(&data);
            done_tx.send(()).expect("signal");
            result.expect("the purge works");
        });
        let early = done_rx.recv_timeout(STILL_WAITING).is_ok();
        // The release comes before any assert, so a failed check cannot leave
        // the render held and the scope waiting for the thread.
        let _ = gate.release.send(());
        let late = early || done_rx.recv_timeout(DRAIN_WAIT).is_ok();
        if !late {
            // A purge that never returns would keep the scope from ending.
            queue.shutdown();
        }
        assert!(!early, "the purge waits for the running render");
        assert!(late, "the purge returns after the release");
    });
    assert_eq!(files_under(&setup.cache_root).len(), 0);
    assert_eq!(setup.db.count_thumbnails().expect("count"), 0);
}

#[test]
fn a_folder_renders_every_file_again_after_a_purge() {
    let calls = Arc::default();
    let mut setup = start(TempDir::new(), 2, counting_render(&calls));
    let folder = open_stored(&mut setup, &["a.png", "b.png"]);
    assert_eq!(calls.lock().expect("calls lock").len(), 2);

    setup
        .pipeline
        .purge(&data_dir(&setup))
        .expect("the purge works");
    setup.pipeline.open_folder(&folder).expect("open again");
    collect(&setup.results, 2);

    assert_eq!(calls.lock().expect("calls lock").len(), 4);
}
