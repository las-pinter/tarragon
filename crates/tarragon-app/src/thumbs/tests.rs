use std::time::Instant;

use image::{DynamicImage, RgbaImage};
use tarragon_cache::store::CacheStore;
use tarragon_core::sorting::SortMode;
use tarragon_db::Database;

use super::*;
use crate::pipeline::Config;

/// A temp folder that removes itself on drop.
struct TempDir(PathBuf);

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn a_folder_with_a_real_pipeline_settles_all_tiles() {
    let dir = TempDir(std::env::temp_dir().join(format!("tarragon-thumbs-{}", std::process::id())));
    let folder = dir.0.join("pics");
    std::fs::create_dir_all(&folder).expect("make the folder");
    for name in ["a.png", "b.png", "c.png"] {
        RgbaImage::new(8, 8)
            .save(folder.join(name))
            .expect("write a png");
    }
    let db = Arc::new(Database::open(dir.0.join("t.db")).expect("open the db"));
    db.initialize().expect("initialize the db");
    let config = Config {
        cache: CacheStore::new(dir.0.join("cache")),
        workers: 2,
        render: Arc::new(|_| Ok(DynamicImage::ImageRgba8(RgbaImage::new(8, 8)))),
    };
    let repaint: Repaint = Arc::new(|| {});
    let (pipeline, results) =
        Pipeline::new(config, db, Arc::clone(&repaint)).expect("start the pipeline");
    let mut thumbs = Thumbs::new(pipeline, results, Some(folder), repaint);
    let ctx = Context::default();
    let mut gallery = Gallery::new(SortMode::Natural);

    // The first frame starts the open. The workers then finish while the
    // frame loop has not taken the answer yet.
    thumbs.before_draw(&ctx, &mut gallery);
    assert!(thumbs.loaded_once().is_none(), "no tile is settled yet");
    let start = Instant::now();
    let summary = loop {
        thumbs.before_draw(&ctx, &mut gallery);
        if let Some(summary) = thumbs.loaded_once() {
            break summary;
        }
        assert!(
            start.elapsed() < Duration::from_secs(30),
            "the grid never settled"
        );
        std::thread::sleep(Duration::from_millis(10));
    };
    assert!(
        summary.starts_with("3 tiles settled, 0 failed"),
        "{summary}"
    );
    assert_eq!(gallery.visible_len(), 3);
}

/// A thumbs on a real pipeline with a fake renderer, and a folder tree to fill.
struct Rig {
    dir: TempDir,
    thumbs: Thumbs,
    ctx: Context,
    gallery: Gallery,
}

impl Rig {
    fn new(name: &str, start: Option<&str>) -> Self {
        let dir = TempDir(
            std::env::temp_dir().join(format!("tarragon-thumbs-{name}-{}", std::process::id())),
        );
        std::fs::create_dir_all(&dir.0).expect("make the temp dir");
        let db = Arc::new(Database::open(dir.0.join("t.db")).expect("open the db"));
        db.initialize().expect("initialize the db");
        let config = Config {
            cache: CacheStore::new(dir.0.join("cache")),
            workers: 2,
            render: Arc::new(|_| Ok(DynamicImage::ImageRgba8(RgbaImage::new(8, 8)))),
        };
        let repaint: Repaint = Arc::new(|| {});
        let (pipeline, results) =
            Pipeline::new(config, db, Arc::clone(&repaint)).expect("start the pipeline");
        let mut rig = Self {
            thumbs: Thumbs::new(pipeline, results, None, repaint),
            ctx: Context::default(),
            gallery: Gallery::new(SortMode::Natural),
            dir,
        };
        if let Some(start) = start {
            let folder = rig.folder(start, &["s.png"]);
            rig.thumbs.navigate(folder);
        }
        rig
    }

    /// Make the folder `name` with the given pngs.
    fn folder(&self, name: &str, files: &[&str]) -> PathBuf {
        let folder = self.dir.0.join(name);
        std::fs::create_dir_all(&folder).expect("make the folder");
        for file in files {
            RgbaImage::new(8, 8)
                .save(folder.join(file))
                .expect("write a png");
        }
        folder
    }

    /// Run frames until the open is over and every tile settled.
    fn settle(&mut self) -> String {
        let start = Instant::now();
        loop {
            self.thumbs.before_draw(&self.ctx, &mut self.gallery);
            if let Some(summary) = self.thumbs.loaded_once() {
                return summary;
            }
            assert!(
                start.elapsed() < Duration::from_secs(30),
                "the grid never settled"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// Run frames until no open runs or waits.
    fn run_until_idle(&mut self) {
        let start = Instant::now();
        while !self.thumbs.is_idle() {
            self.thumbs.before_draw(&self.ctx, &mut self.gallery);
            assert!(start.elapsed() < Duration::from_secs(30), "the open hangs");
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

#[test]
fn navigate_without_a_start_folder_reaches_the_tiles() {
    let mut rig = Rig::new("none", None);
    let folder = rig.folder("two", &["a.png", "b.png"]);
    rig.thumbs.navigate(folder.clone());
    let summary = rig.settle();
    assert!(
        summary.starts_with("2 tiles settled, 0 failed"),
        "{summary}"
    );
    assert_eq!(rig.gallery.visible_len(), 2);
    assert_eq!(rig.thumbs.take_folder_change(), Some(folder));
    assert_eq!(rig.thumbs.take_folder_change(), None);
}

#[test]
fn navigate_from_a_to_b_leaves_only_b() {
    let mut rig = Rig::new("ab", None);
    let a = rig.folder("a", &["a1.png", "a2.png", "a3.png"]);
    let b = rig.folder("b", &["b1.png", "b2.png"]);
    rig.thumbs.navigate(a.clone());
    assert!(rig.settle().starts_with("3 tiles settled"));
    rig.thumbs.navigate(b.clone());
    assert!(rig.settle().starts_with("2 tiles settled, 0 failed"));
    assert_eq!(rig.gallery.visible_len(), 2);
    assert!(rig.gallery.has_item(&b.join("b1.png")));
    assert!(rig.gallery.has_item(&b.join("b2.png")));
    assert!(!rig.gallery.has_item(&a.join("a1.png")));
    assert_eq!(rig.gallery.folder(), Some(b.as_path()));
}

#[test]
fn two_navigates_before_one_frame_open_the_last() {
    let mut rig = Rig::new("last", None);
    let a = rig.folder("a", &["a1.png", "a2.png", "a3.png"]);
    let b = rig.folder("b", &["b1.png"]);
    rig.thumbs.navigate(a);
    rig.thumbs.navigate(b.clone());
    assert!(rig.settle().starts_with("1 tiles settled"));
    assert_eq!(rig.gallery.folder(), Some(b.as_path()));
    assert_eq!(rig.gallery.visible_len(), 1);
}

#[test]
fn a_navigate_during_an_open_waits_and_wins() {
    let mut rig = Rig::new("busy", None);
    let a = rig.folder("a", &["a1.png", "a2.png"]);
    let b = rig.folder("b", &["b1.png"]);
    let c = rig.folder("c", &["c1.png", "c2.png", "c3.png"]);
    rig.thumbs.navigate(a);
    rig.thumbs.before_draw(&rig.ctx, &mut rig.gallery);
    rig.thumbs.navigate(b);
    rig.thumbs.navigate(c.clone());
    rig.run_until_idle();
    assert!(rig.settle().starts_with("3 tiles settled"));
    assert_eq!(rig.gallery.folder(), Some(c.as_path()));
}

#[test]
fn a_missing_folder_keeps_the_old_one() {
    let mut rig = Rig::new("missing", None);
    let a = rig.folder("a", &["a1.png", "a2.png"]);
    rig.thumbs.navigate(a.clone());
    rig.settle();
    assert_eq!(rig.thumbs.take_folder_change(), Some(a.clone()));
    rig.thumbs.navigate(rig.dir.0.join("gone"));
    rig.run_until_idle();
    assert_eq!(rig.thumbs.take_folder_change(), Some(a.clone()));
    assert_eq!(rig.gallery.folder(), Some(a.as_path()));
    assert_eq!(rig.gallery.visible_len(), 2);
}

#[test]
fn navigate_to_the_open_folder_does_nothing() {
    let mut rig = Rig::new("same", Some("one"));
    rig.settle();
    let open = rig.dir.0.join("one");
    rig.thumbs.navigate(open.clone());
    assert!(rig.thumbs.is_idle(), "no rescan is queued");
    // A folder that is already asked for is not asked for twice.
    let other = rig.folder("two", &["t.png"]);
    rig.thumbs.navigate(other.clone());
    rig.thumbs.navigate(other);
    assert!(!rig.thumbs.is_idle());
    rig.settle();
    assert_eq!(rig.gallery.visible_len(), 1);
}

#[test]
fn a_waiting_folder_keeps_the_highlight_until_it_opens() {
    let mut rig = Rig::new("wait", None);
    let a = rig.folder("a", &["a1.png"]);
    let b = rig.folder("b", &["b1.png", "b2.png"]);
    rig.thumbs.navigate(a);
    rig.thumbs.before_draw(&rig.ctx, &mut rig.gallery);
    rig.thumbs.navigate(b.clone());
    let mut changes = Vec::new();
    let start = Instant::now();
    while !rig.thumbs.is_idle() {
        rig.thumbs.before_draw(&rig.ctx, &mut rig.gallery);
        changes.extend(rig.thumbs.take_folder_change());
        assert!(start.elapsed() < Duration::from_secs(30), "the open hangs");
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(changes, [b]);
}

#[test]
fn navigate_to_the_opening_folder_cancels_the_waiting_one() {
    let mut rig = Rig::new("opening", None);
    let a = rig.folder("a", &["a1.png"]);
    let b = rig.folder("b", &["b1.png"]);
    rig.thumbs.navigate(a.clone());
    rig.thumbs.before_draw(&rig.ctx, &mut rig.gallery);
    rig.thumbs.navigate(b);
    rig.thumbs.navigate(a.clone());
    rig.settle();
    assert_eq!(rig.gallery.folder(), Some(a.as_path()));
    assert!(rig.thumbs.is_idle(), "the waiting folder was cancelled");
}

#[test]
fn navigate_to_the_shown_folder_cancels_the_waiting_one() {
    let mut rig = Rig::new("cancel", None);
    let a = rig.folder("a", &["a1.png"]);
    let b = rig.folder("b", &["b1.png"]);
    rig.thumbs.navigate(a.clone());
    rig.settle();
    rig.thumbs.navigate(b);
    assert!(!rig.thumbs.is_idle());
    rig.thumbs.navigate(a.clone());
    assert!(rig.thumbs.is_idle());
    assert_eq!(rig.gallery.folder(), Some(a.as_path()));
}
