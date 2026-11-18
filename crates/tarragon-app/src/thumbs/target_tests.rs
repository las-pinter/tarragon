use image::{DynamicImage, RgbaImage};
use tarragon_cache::store::CacheStore;
use tarragon_core::sorting::SortMode;
use tarragon_db::Database;

use std::sync::Weak;

use super::*;
use crate::pipeline::Config;

/// A thumbs on an empty database, and a weak handle to see when it is gone.
fn thumbs_on_empty_db(name: &str) -> (Thumbs, Weak<Database>, PathBuf) {
    let dir = std::env::temp_dir().join(format!("tarragon-target-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("make the temp dir");
    let db = Arc::new(Database::open(dir.join("t.db")).expect("open the db"));
    db.initialize().expect("initialize the db");
    let config = Config {
        cache: CacheStore::new(dir.join("cache")),
        workers: 1,
        render: Arc::new(|_| Ok(DynamicImage::ImageRgba8(RgbaImage::new(8, 8)))),
    };
    let repaint: Repaint = Arc::new(|| {});
    let (pipeline, results) =
        Pipeline::new(config, Arc::clone(&db), Arc::clone(&repaint)).expect("start the pipeline");
    (
        Thumbs::new(pipeline, results, None, repaint),
        Arc::downgrade(&db),
        dir,
    )
}

/// Stop a thumbs while its open thread runs, and check the join happened.
fn shutdown_during(target: Target, name: &str) {
    let (mut thumbs, db, dir) = thumbs_on_empty_db(name);
    thumbs.start_open(target);
    assert!(matches!(thumbs.slot, Slot::Opening { .. }));
    let wait = ExitWait::default();
    thumbs.shutdown(&wait, None);
    assert!(wait.0.lock().expect("lock").is_some(), "no handle to join");
    wait.wait();
    // Nobody owns the database once the pipeline is dropped.
    assert!(db.upgrade().is_none(), "the pipeline outlived the join");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn shutdown_joins_an_opening_folder() {
    let dir = std::env::temp_dir().join(format!("tarragon-target-dir-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("make the folder");
    shutdown_during(Target::Folder(dir.clone()), "folder");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn shutdown_joins_an_opening_library() {
    shutdown_during(Target::Library, "library");
}

#[test]
fn a_library_open_leaves_the_folder_and_the_flags_alone() {
    let (mut thumbs, _db, dir) = thumbs_on_empty_db("library-arm");
    let ctx = Context::default();
    let mut gallery = Gallery::new(SortMode::Natural);
    thumbs.start_open(Target::Library);
    let start = Instant::now();
    while matches!(thumbs.slot, Slot::Opening { .. }) {
        thumbs.before_draw(&ctx, &mut gallery);
        assert!(start.elapsed() < Duration::from_secs(30), "the open hangs");
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(matches!(thumbs.slot, Slot::Idle(_)));
    assert_eq!(thumbs.take_folder_change(), None);
    assert_eq!(gallery.folder(), None);
    assert!(thumbs.library_loaded());
    assert!(!thumbs.opened);
    let _ = std::fs::remove_dir_all(dir);
}
