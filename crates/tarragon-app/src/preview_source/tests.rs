//! Tests for the Preview source. They use a real database in a temp folder.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use image::{Rgba, RgbaImage};
use tarragon_db::Database;
use tarragon_db::repository::ThumbnailWrite;

use super::read;

/// A temp folder that removes itself on drop.
struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "tarragon-preview-source-{}-{id}",
            std::process::id()
        ));
        std::fs::create_dir_all(&path).expect("make the temp folder");
        Self(path)
    }

    fn db(&self) -> Database {
        let db = Database::open(self.0.join("t.db")).expect("open");
        db.initialize().expect("initialize");
        db
    }

    /// Write a PNG of `size` and return its path.
    fn png(&self, name: &str, size: (u32, u32)) -> PathBuf {
        let path = self.0.join(name);
        RgbaImage::from_pixel(size.0, size.1, Rgba([1, 2, 3, 255]))
            .save(&path)
            .expect("write a png");
        path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

/// Store a row for `path` with the given tier files and a 2000 x 1000 original.
fn store(db: &Database, path: &Path, small: Option<&Path>, big: Option<&Path>) {
    let record = ThumbnailWrite {
        path: path.to_string_lossy().into_owned(),
        mtime: 1,
        size: 4321,
        width: 2000,
        height: 1000,
        cache_uuid: "u".into(),
        thumbnail_cache_path: small.map(text),
        preview_cache_path: big.map(text),
        full_cache_path: None,
        last_accessed: 1,
    };
    db.upsert_thumbnail(&record).expect("store the row");
}

/// A rendered file: the source, a 320 tier, and a 1024 tier.
fn rendered(dir: &TempDir, db: &Database, name: &str) -> PathBuf {
    let source = dir.0.join(name);
    std::fs::write(&source, b"x").expect("write the source");
    let small = dir.png(&format!("s-{name}"), (320, 160));
    let big = dir.png(&format!("b-{name}"), (1024, 512));
    store(db, &source, Some(&small), Some(&big));
    source
}

#[test]
fn one_rendered_png_gives_its_details_and_one_item() {
    let dir = TempDir::new();
    let db = dir.db();
    let path = rendered(&dir, &db, "cat.png");
    let source = read(&db, std::slice::from_ref(&path), 4);
    let meta = source.meta.expect("a meta for one file");
    assert_eq!(meta.name, "cat.png");
    assert_eq!(meta.format, "PNG");
    assert_eq!(meta.dimensions, Some((2000, 1000)));
    assert_eq!(meta.size, Some(4321));
    assert_eq!(source.items.len(), 1);
    assert_eq!(source.items[0].key, path);
    assert_eq!(source.items[0].size, (1024, 512));
    assert_eq!(source.items[0].tier, dir.0.join("b-cat.png"));
}

#[test]
fn two_selected_paths_give_no_meta_and_two_items() {
    let dir = TempDir::new();
    let db = dir.db();
    let paths = [rendered(&dir, &db, "a.png"), rendered(&dir, &db, "b.png")];
    let source = read(&db, &paths, 4);
    assert!(source.meta.is_none());
    assert_eq!(source.total, 2);
    assert_eq!(source.items.len(), 2);
}

#[test]
fn the_cap_limits_the_items_but_not_the_total() {
    let dir = TempDir::new();
    let db = dir.db();
    let paths: Vec<PathBuf> = (0..5)
        .map(|n| rendered(&dir, &db, &format!("{n}.png")))
        .collect();
    let source = read(&db, &paths, 3);
    assert_eq!(source.items.len(), 3);
    assert_eq!(source.total, 5);
    assert_eq!(source.items[2].key, paths[2]);
}

#[test]
fn a_path_with_no_row_gives_a_meta_from_the_file_and_no_item() {
    let dir = TempDir::new();
    let db = dir.db();
    let path = dir.0.join("bare");
    std::fs::write(&path, b"12345").expect("write the file");
    let source = read(&db, std::slice::from_ref(&path), 4);
    let meta = source.meta.expect("a meta even with no row");
    assert_eq!(meta.size, Some(5));
    assert_eq!(meta.dimensions, None);
    assert_eq!(meta.format, "Unknown");
    assert!(source.items.is_empty());
}

#[test]
fn a_missing_preview_tier_falls_back_to_the_320_file() {
    let dir = TempDir::new();
    let db = dir.db();
    let path = dir.0.join("c.png");
    let small = dir.png("small.png", (320, 160));
    store(&db, &path, Some(&small), Some(&dir.0.join("gone.png")));
    let source = read(&db, std::slice::from_ref(&path), 4);
    assert_eq!(source.items.len(), 1);
    assert_eq!(source.items[0].tier, small);
}

#[test]
fn a_row_with_no_tier_file_gives_no_item() {
    let dir = TempDir::new();
    let db = dir.db();
    let path = dir.0.join("d.png");
    store(&db, &path, None, Some(&dir.0.join("gone.png")));
    assert!(read(&db, &[path], 4).items.is_empty());
}

#[test]
fn a_zero_size_row_takes_the_size_of_the_tier_file() {
    let dir = TempDir::new();
    let db = dir.db();
    let path = dir.0.join("e.png");
    let small = dir.png("e-small.png", (320, 100));
    let record = ThumbnailWrite {
        path: path.to_string_lossy().into_owned(),
        mtime: 1,
        size: 1,
        width: 0,
        height: 0,
        cache_uuid: "u".into(),
        thumbnail_cache_path: Some(text(&small)),
        preview_cache_path: None,
        full_cache_path: None,
        last_accessed: 1,
    };
    db.upsert_thumbnail(&record).expect("store the row");
    let source = read(&db, std::slice::from_ref(&path), 4);
    assert_eq!(source.items[0].size, (320, 100));
    assert_eq!(source.meta.expect("meta").dimensions, None);
}
