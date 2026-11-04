//! Tests for the eviction at start and the `full` tier clear at exit.

use std::time::Duration;

use tarragon_db::repository::ThumbnailWrite;

use super::tests::{TempDir, new_dir};
use super::*;
use crate::thumbs::{Work, WorkOutcome};

/// The size of each tier file of an evictable source. The files must be
/// sparse: the smallest cap is 1 GiB, and real files of this size would fill
/// the disk. Three sources of three files weigh 1.8 GiB.
const TIER_BYTES: u64 = 200 << 20;

/// How long the eviction is given to finish before the test fails.
const STEP_WAIT: Duration = Duration::from_secs(10);

/// The three tier files of the source `name` below `root`.
fn tier_paths(root: &Path, name: &str) -> [PathBuf; 3] {
    ["256", "1024", "full"].map(|tier| root.join(tier).join("x").join(format!("{name}.jpg")))
}

/// Write the tier files of `name` with `len` bytes each.
fn write_tiers(root: &Path, name: &str, len: u64) -> [PathBuf; 3] {
    let paths = tier_paths(root, name);
    for path in &paths {
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("make a tier folder");
        let file = std::fs::File::create(path).expect("create a tier file");
        file.set_len(len).expect("size a tier file");
    }
    paths
}

/// Store a row for `name` that points at its tier files.
fn add_row(db: &Database, root: &Path, name: &str, last_accessed: i64) {
    let paths = write_tiers(root, name, TIER_BYTES);
    let text = |path: &PathBuf| Some(path.to_string_lossy().into_owned());
    db.upsert_thumbnail(&ThumbnailWrite {
        path: format!("/pics/{name}.png"),
        mtime: 1,
        size: 1,
        width: 1,
        height: 1,
        cache_uuid: "x".to_owned(),
        thumbnail_cache_path: text(&paths[0]),
        preview_cache_path: text(&paths[1]),
        full_cache_path: text(&paths[2]),
        last_accessed,
    })
    .expect("store a row");
}

/// Prepare the database in `dir` before the app opens it.
fn prepare(dir: &TempDir, fill: impl FnOnce(&Database)) {
    let db = Database::open(dir.0.join("t.db")).expect("open the db");
    db.initialize().expect("initialize the db");
    fill(&db);
}

/// Open the app on the database in `dir`, with a wait handle the test keeps.
fn open_app(dir: &TempDir) -> (App, ExitWait) {
    let (_filter, control) = LogControl::new_filter();
    let exit_wait = ExitWait::default();
    let startup = Startup {
        folder: None,
        close_when_loaded: false,
        exit_wait: exit_wait.clone(),
    };
    let app = App::open(&dir.0.join("t.db"), LogRing::new(), control, None, startup)
        .expect("open the app");
    (app, exit_wait)
}

/// Run frames until the start eviction reports.
fn evicted_at_start(app: &mut App) -> usize {
    let ctx = Context::default();
    let start = Instant::now();
    loop {
        app.thumbs.before_draw(&ctx, app.shell.gallery_mut());
        if let Some(result) = app.thumbs.take_work_result() {
            let (Work::Evict, Ok(WorkOutcome::Evicted(done))) = result else {
                panic!("the start work gave {result:?}");
            };
            return done.rows;
        }
        assert!(start.elapsed() < STEP_WAIT, "the eviction never finished");
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// Close the app the way the window does, and wait for the exit thread.
fn exit(app: &mut App, wait: &ExitWait) {
    eframe::App::on_exit(app, None);
    wait.wait();
}

/// Set the setting `key` to `value`.
fn set(db: &Database, key: SettingKey, value: SettingValue) {
    Settings::new(db).set(key, value).expect("write a setting");
}

#[test]
fn start_evicts_the_oldest_sources_with_their_rows() {
    let dir = new_dir();
    let cache = dir.0.join("cache");
    prepare(&dir, |db| {
        add_row(db, &cache, "mid", 20);
        add_row(db, &cache, "old", 10);
        add_row(db, &cache, "new", 30);
        set(db, SettingKey::CacheSizeCapGb, SettingValue::Int(1));
    });
    let (mut app, _wait) = open_app(&dir);

    assert_eq!(evicted_at_start(&mut app), 2);

    for name in ["old", "mid"] {
        assert!(tier_paths(&cache, name).iter().all(|p| !p.exists()));
        let row = app.db.thumbnail(&format!("/pics/{name}.png"));
        assert!(row.expect("read").is_none(), "{name} kept its row");
    }
    assert!(tier_paths(&cache, "new").iter().all(|p| p.exists()));
    let row = app.db.thumbnail("/pics/new.png");
    assert!(row.expect("read").is_some(), "new lost its row");
}

#[test]
fn start_with_a_zero_cap_evicts_nothing() {
    let dir = new_dir();
    let cache = dir.0.join("cache");
    prepare(&dir, |db| {
        add_row(db, &cache, "old", 10);
        add_row(db, &cache, "new", 30);
        set(db, SettingKey::CacheSizeCapGb, SettingValue::Int(0));
    });
    let (mut app, _wait) = open_app(&dir);

    assert_eq!(evicted_at_start(&mut app), 0);

    for name in ["old", "new"] {
        assert!(tier_paths(&cache, name).iter().all(|p| p.exists()));
    }
    assert_eq!(app.db.count_thumbnails().expect("count"), 2);
}

#[test]
fn start_with_the_default_cap_keeps_a_small_cache() {
    let dir = new_dir();
    let cache = dir.0.join("cache");
    prepare(&dir, |db| add_row(db, &cache, "old", 10));
    let (mut app, _wait) = open_app(&dir);

    assert_eq!(evicted_at_start(&mut app), 0);
    assert!(tier_paths(&cache, "old").iter().all(|p| p.exists()));
}

#[test]
fn exit_clears_only_the_full_tier_when_the_setting_is_on() {
    let dir = new_dir();
    let cache = dir.0.join("cache");
    let [small, medium, full] = write_tiers(&cache, "a", 10);
    prepare(&dir, |db| {
        set(db, SettingKey::ClearFullResOnExit, SettingValue::Bool(true));
    });
    let (mut app, wait) = open_app(&dir);

    exit(&mut app, &wait);

    assert!(!full.exists(), "the full tier file stayed");
    assert!(
        small.exists() && medium.exists(),
        "a smaller tier was cleared"
    );
}

#[test]
fn exit_removes_no_file_when_the_setting_is_off() {
    let dir = new_dir();
    let cache = dir.0.join("cache");
    let tiers = write_tiers(&cache, "a", 10);
    prepare(&dir, |db| {
        set(
            db,
            SettingKey::ClearFullResOnExit,
            SettingValue::Bool(false),
        );
    });
    let (mut app, wait) = open_app(&dir);

    exit(&mut app, &wait);

    assert!(tiers.iter().all(|p| p.exists()));
}

#[test]
fn exit_refuses_to_clear_a_cache_that_is_the_data_folder() {
    let dir = new_dir();
    let tiers = write_tiers(&dir.0, "a", 10);
    prepare(&dir, |db| {
        let here = dir.0.to_string_lossy().into_owned();
        set(db, SettingKey::CacheDir, SettingValue::Text(here));
        set(db, SettingKey::ClearFullResOnExit, SettingValue::Bool(true));
    });
    let (mut app, wait) = open_app(&dir);

    exit(&mut app, &wait);

    assert!(
        tiers.iter().all(|p| p.exists()),
        "the clear was not refused"
    );
    assert!(dir.0.join("t.db").exists(), "the database is gone");
}

#[test]
fn the_cap_reads_an_int_and_falls_back_to_the_default() {
    let read = |value: Result<SettingValue, &str>| cap_or_default(value);
    assert_eq!(read(Ok(SettingValue::Int(7))), 7);
    assert_eq!(read(Ok(SettingValue::Text("x".to_owned()))), 20);
    assert_eq!(read(Err("broken")), 20);
}

#[test]
fn only_a_true_setting_clears_the_full_tier() {
    let read = |value: Result<SettingValue, &str>| clear_flag(value);
    assert!(read(Ok(SettingValue::Bool(true))));
    assert!(!read(Ok(SettingValue::Bool(false))));
    assert!(!read(Ok(SettingValue::Int(1))));
    assert!(!read(Err("broken")));
}
