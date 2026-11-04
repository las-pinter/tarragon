//! Tests for the cache eviction caller.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use tarragon_db::repository::ThumbnailWrite;

use super::*;

/// The size of each tier file, in bytes. One source holds three of them.
const TIER_BYTES: usize = 100_000;

/// The size of one source (three tier files), in bytes.
const SOURCE_BYTES: u64 = 3 * TIER_BYTES as u64;

/// A temp folder that removes itself on drop.
struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "tarragon-maintenance-test-{}-{id}",
            std::process::id()
        ));
        std::fs::create_dir_all(&path).expect("make the temp folder");
        Self(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A database and a cache in a temp folder.
struct Setup {
    db: Database,
    cache: CacheStore,
    root: PathBuf,
    // Declared last so the database closes before the folder goes.
    _dir: TempDir,
}

fn setup() -> Setup {
    let dir = TempDir::new();
    let db = Database::open(dir.0.join("test.db")).expect("open the db");
    db.initialize().expect("initialize the db");
    let root = dir.0.join("cache");
    std::fs::create_dir_all(&root).expect("make the cache root");
    Setup {
        db,
        cache: CacheStore::new(&root),
        root,
        _dir: dir,
    }
}

/// The three tier file paths of the source `name`.
fn tier_paths(root: &Path, name: &str) -> [PathBuf; 3] {
    ["256", "1024", "full"].map(|tier| root.join(tier).join("folder_x").join(format!("{name}.jpg")))
}

/// Write the three tier files of `name` and a row with `last_accessed`.
fn add_source(setup: &Setup, name: &str, last_accessed: i64) {
    let paths = tier_paths(&setup.root, name);
    for path in &paths {
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("make a tier folder");
        std::fs::write(path, vec![7u8; TIER_BYTES]).expect("write a tier file");
    }
    let text = |path: &PathBuf| Some(path.to_string_lossy().into_owned());
    setup
        .db
        .upsert_thumbnail(&ThumbnailWrite {
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

/// Add three sources; `old` is the least recently accessed.
fn add_three(setup: &Setup) {
    add_source(setup, "mid", 20);
    add_source(setup, "old", 10);
    add_source(setup, "new", 30);
}

/// True when every tier file of `name` exists.
fn all_exist(setup: &Setup, name: &str) -> bool {
    tier_paths(&setup.root, name).iter().all(|p| p.exists())
}

/// True when no tier file of `name` exists.
fn none_exist(setup: &Setup, name: &str) -> bool {
    tier_paths(&setup.root, name).iter().all(|p| !p.exists())
}

#[test]
fn a_zero_cap_evicts_nothing() {
    let setup = setup();
    add_three(&setup);
    let done = evict(&setup.db, &setup.cache, 0).expect("evict");
    assert_eq!(done.rows, 0);
    assert_eq!(done.remaining_bytes, 3 * SOURCE_BYTES);
    assert_eq!(setup.db.count_thumbnails().expect("count"), 3);
    for name in ["old", "mid", "new"] {
        assert!(all_exist(&setup, name), "{name} lost files");
    }
}

#[test]
fn the_oldest_source_goes_first_with_its_row() {
    let setup = setup();
    add_three(&setup);
    let done = evict_bytes(&setup.db, &setup.cache, 2 * SOURCE_BYTES).expect("evict");
    assert_eq!(done.rows, 1);
    assert_eq!(done.remaining_bytes, 2 * SOURCE_BYTES);
    assert!(none_exist(&setup, "old"));
    assert!(setup.db.thumbnail("/pics/old.png").expect("read").is_none());
    for name in ["mid", "new"] {
        assert!(all_exist(&setup, name), "{name} lost files");
        let row = setup.db.thumbnail(&format!("/pics/{name}.png"));
        assert!(row.expect("read").is_some(), "{name} lost its row");
    }
    assert_eq!(setup.db.count_thumbnails().expect("count"), 2);
}

#[test]
fn the_cap_converts_from_gib() {
    assert_eq!(cap_to_bytes(20), 20 * (1u64 << 30));
    assert_eq!(cap_to_bytes(20), 21_474_836_480);
    assert_eq!(cap_to_bytes(0), 0);
    assert_eq!(cap_to_bytes(-5), 0);
    assert_eq!(cap_to_bytes(i64::MAX), u64::MAX);
}

#[test]
fn a_negative_cap_evicts_nothing() {
    let setup = setup();
    add_three(&setup);
    let done = evict(&setup.db, &setup.cache, -1).expect("evict");
    assert_eq!(done.rows, 0);
    assert_eq!(setup.db.count_thumbnails().expect("count"), 3);
}

#[test]
fn a_cache_under_the_cap_keeps_everything() {
    let setup = setup();
    add_three(&setup);
    let done = evict(&setup.db, &setup.cache, 1).expect("evict");
    assert_eq!(done.rows, 0);
    assert_eq!(setup.db.count_thumbnails().expect("count"), 3);
}

#[test]
fn a_row_without_cache_paths_is_never_deleted() {
    let setup = setup();
    add_three(&setup);
    // The oldest row owns no files; it must not be evicted or deleted.
    setup
        .db
        .upsert_thumbnail(&ThumbnailWrite {
            path: "/pics/bare.png".to_owned(),
            mtime: 1,
            size: 1,
            width: 1,
            height: 1,
            cache_uuid: "x".to_owned(),
            thumbnail_cache_path: None,
            preview_cache_path: None,
            full_cache_path: None,
            last_accessed: 0,
        })
        .expect("store a bare row");
    let done = evict_bytes(&setup.db, &setup.cache, SOURCE_BYTES).expect("evict");
    assert_eq!(done.rows, 2);
    assert!(
        setup
            .db
            .thumbnail("/pics/bare.png")
            .expect("read")
            .is_some()
    );
    // The bare row and the newest source stay.
    assert_eq!(setup.db.count_thumbnails().expect("count"), 2);
    assert!(all_exist(&setup, "new"));
}
