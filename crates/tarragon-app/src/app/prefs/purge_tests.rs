//! Tests of the Cache Size row and the Purge Cache button. Every frame is
//! headless; the database and the cache sit in a temp folder.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crossbeam_channel::Sender;
use image::{DynamicImage, RgbaImage};
use tarragon_cache::store::CacheStore;
use tarragon_ui::gallery::Scope;
use tarragon_ui::preview::size_text;
use time::UtcOffset;
use tracing_subscriber::layer::SubscriberExt;

use super::super::preview_tests::Rig;
use super::purge::{lock, shared};
use super::*;
use crate::logging::LogControl;
use crate::logging::ring::{LogRing, RingLayer};
use crate::pipeline::{Config, Pipeline, Repaint};
use crate::thumbs::{Thumbs, Work};

/// How long a step that must finish is given before the test fails.
const LIMIT: Duration = Duration::from_secs(10);

/// A rig whose renders can be held. While armed, a render waits for the
/// release, or gives up after [`LIMIT`], so a blocked frame loop fails the
/// test instead of hanging it. Dropping the rig releases the renders.
struct Held {
    rig: Rig,
    armed: Arc<AtomicBool>,
    entered: crossbeam_channel::Receiver<()>,
    release: Option<Sender<()>>,
    folder: PathBuf,
}

impl Held {
    /// A rig with three pictures to open. The cache sits in its own folder, or
    /// in the data folder when `cache_in_data` is set.
    fn new(cache_in_data: bool) -> Self {
        let mut rig = Rig::new(&[], &[], 9);
        let folder = rig.dir.0.join("pics");
        std::fs::create_dir_all(&folder).expect("make the folder");
        for name in ["a.png", "b.png", "c.png"] {
            RgbaImage::new(8, 8)
                .save(folder.join(name))
                .expect("write a png");
        }
        let armed = Arc::new(AtomicBool::new(false));
        let (entered_tx, entered) = crossbeam_channel::unbounded::<()>();
        let (release, release_rx) = crossbeam_channel::unbounded::<()>();
        let gate = Arc::clone(&armed);
        let render: crate::pipeline::RenderFn = Arc::new(move |_| {
            if gate.load(Ordering::SeqCst) {
                let _ = entered_tx.send(());
                // Dropping the sender is the release; the timeout is the backstop.
                let _ = release_rx.recv_timeout(LIMIT);
            }
            Ok(DynamicImage::ImageRgba8(RgbaImage::new(8, 8)))
        });
        let cache = if cache_in_data {
            rig.dir.0.clone()
        } else {
            rig.dir.0.join("cache")
        };
        let config = Config {
            cache: CacheStore::new(cache.clone()),
            workers: 2,
            render,
        };
        let repaint: Repaint = Arc::new(|| {});
        let (pipeline, results) =
            Pipeline::new(config, Arc::clone(&rig.app.db), Arc::clone(&repaint))
                .expect("start the pipeline");
        rig.app.thumbs = Thumbs::new(pipeline, results, None, repaint);
        // The folder view is under test, not the library view.
        rig.app.shell.gallery_mut().set_scope(Scope::Folder);
        rig.app.thumbs.skip_library();
        rig.app.cache_root = cache;
        Self {
            rig,
            armed,
            entered,
            release: Some(release),
            folder,
        }
    }

    fn arm(&self) {
        self.armed.store(true, Ordering::SeqCst);
    }

    /// Let every held and later render go on.
    fn release(&mut self) {
        self.armed.store(false, Ordering::SeqCst);
        self.release.take();
    }

    /// Draw frames until `done` holds for the texts of a frame. The frame
    /// loop fails after [`LIMIT`].
    fn until(&mut self, what: &str, mut done: impl FnMut(&mut Rig, &[String]) -> bool) {
        let start = Instant::now();
        loop {
            let texts = self.rig.frame(Vec::new()).0;
            if done(&mut self.rig, &texts) {
                return;
            }
            assert!(start.elapsed() < LIMIT, "never happened: {what}");
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// Open the folder and draw until every tile is settled.
    fn open_and_settle(&mut self) {
        self.rig.app.navigate(&self.folder);
        self.until("the tiles settle", |rig, _| {
            rig.app.thumbs.loaded_once().is_some()
        });
        self.until("the rows are stored", |rig, _| {
            rig.app.db.count_thumbnails().is_ok_and(|rows| rows == 3)
        });
    }

    /// Open the folder with the renders held, and wait until one is held.
    fn open_held(&mut self) {
        self.arm();
        self.rig.app.navigate(&self.folder);
        let entered = self.entered.clone();
        self.until("a render is held", |_, _| entered.try_recv().is_ok());
    }

    /// Open the window from the menu and draw until the form has its buttons.
    fn open_window(&mut self) {
        open_from_menu(&mut self.rig);
        self.until("the form shows", |_, texts| {
            texts.iter().any(|t| t == "Purge Cache")
        });
    }

    /// Draw until the purge has finished in the app.
    fn until_purged(&mut self) {
        self.until("the purge ends", |rig, _| !lock(&shared(&rig.ctx)).purging);
    }
}

impl Drop for Held {
    fn drop(&mut self) {
        self.release();
    }
}

/// Choose Settings, Preferences... as the user does.
fn open_from_menu(rig: &mut Rig) {
    rig.frame(Vec::new());
    rig.click("Settings", false);
    rig.frame(Vec::new());
    rig.click("Preferences...", false);
    assert!(rig.app.shell.prefs_mut().is_open());
}

/// Write files of the given sizes under `dir`, in a subfolder like the cache.
fn fill_cache(dir: &Path, sizes: &[usize]) {
    let sub = dir.join("ab");
    std::fs::create_dir_all(&sub).expect("make the cache folder");
    for (index, size) in sizes.iter().enumerate() {
        std::fs::write(sub.join(format!("f{index}.jpg")), vec![0u8; *size]).expect("write a file");
    }
}

fn files_under(dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.is_dir() {
            found.extend(files_under(&path));
        } else {
            found.push(path);
        }
    }
    found
}

fn shows(texts: &[String], text: &str) -> bool {
    texts.iter().any(|t| t.contains(text))
}

#[test]
fn opening_the_window_shows_the_size_of_the_cache_files() {
    let mut rig = Rig::new(&[], &[], 9);
    fill_cache(&rig.app.cache_root, &[1000, 2000, 3000]);
    open_from_menu(&mut rig);
    let want = size_text(6000);
    let end = Instant::now() + LIMIT;
    while !shows(&rig.frame(Vec::new()).0, &want) {
        assert!(Instant::now() < end, "the size {want} never showed");
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn a_cache_that_cannot_be_read_shows_unknown_and_logs_a_warning() {
    let mut rig = Rig::new(&[], &[], 9);
    // A plain file where the cache folder should be makes the walk fail.
    let plain = rig.dir.0.join("plain");
    std::fs::write(&plain, b"x").expect("write the file");
    rig.app.cache_root = plain;
    let (filter, control) = LogControl::new_filter();
    rig.app.control = control;
    let lines_ring = LogRing::new();
    let subscriber = tracing_subscriber::registry()
        .with(filter)
        .with(RingLayer::new(lines_ring.clone(), UtcOffset::UTC));
    tracing::subscriber::with_default(subscriber, || {
        open_from_menu(&mut rig);
        let end = Instant::now() + LIMIT;
        while lock(&shared(&rig.ctx)).measuring.is_some() {
            rig.frame(Vec::new());
            assert!(Instant::now() < end, "the measure never ended");
            std::thread::sleep(Duration::from_millis(10));
        }
    });
    for _ in 0..4 {
        rig.frame(Vec::new());
    }
    assert!(shows(&rig.frame(Vec::new()).0, CACHE_SIZE_UNKNOWN));
    let lines: Vec<String> = lines_ring.snapshot().into_iter().map(|e| e.text).collect();
    assert!(
        lines
            .iter()
            .any(|line| line.contains("cannot measure the cache size")),
        "{lines:?}"
    );
}

#[test]
fn a_purge_empties_the_cache_and_the_rows_and_the_folder_renders_again() {
    let mut held = Held::new(false);
    held.open_and_settle();
    let cache = held.rig.app.cache_root.clone();
    assert!(!files_under(&cache).is_empty(), "the open filled the cache");
    let before = CacheStore::new(cache.clone())
        .total_size()
        .expect("measure");
    held.open_window();
    let want = size_text(before);
    held.until("the size shows", |_, texts| shows(texts, &want));

    let rows = held.rig.app.db.thumbnails_by_last_accessed().expect("rows");
    assert!(rows.iter().any(|row| row.thumbnail_cache_path.is_some()));

    // The reopened tiles wait at the gate, so the empty state can be checked.
    held.arm();
    held.rig.click("Purge Cache", false);
    held.until_purged();
    assert!(files_under(&cache).is_empty(), "no tier file is left");
    // The reopen scans the folder at once and stores fresh rows. None of them
    // points at a tier file, because the renders are still held.
    let rows = held.rig.app.db.thumbnails_by_last_accessed().expect("rows");
    assert!(rows.iter().all(|row| row.thumbnail_cache_path.is_none()));
    held.until("the size reads zero", |_, texts| shows(texts, "0 B"));
    assert!(!shows(&held.rig.frame(Vec::new()).0, "Purging cache..."));

    held.release();
    held.until("the folder settles again", |rig, _| {
        rig.app.thumbs.loaded_once().is_some()
    });
    assert!(!files_under(&cache).is_empty(), "the tiles rendered again");
}

#[test]
fn a_purge_request_while_other_work_runs_is_ignored() {
    let mut held = Held::new(false);
    held.open_held();
    let data = held.rig.app.data_root.clone();
    held.rig.app.thumbs.purge(data);
    held.until("the purge runs", |rig, _| {
        rig.app.thumbs.is_working() == Some(Work::Purge)
    });
    assert!(!held.rig.app.thumbs.has_queued_work());
    let ctx = held.rig.ctx.clone();
    held.rig.app.purge_cache(&ctx);
    assert!(
        !held.rig.app.thumbs.has_queued_work(),
        "no second purge waits"
    );
    assert!(!lock(&shared(&ctx)).purging, "the state did not change");
}

#[test]
fn a_cache_in_the_data_folder_shows_an_error_and_keeps_the_database() {
    let mut held = Held::new(true);
    held.open_and_settle();
    held.open_window();
    held.rig.click("Purge Cache", false);
    held.until_purged();
    held.until("the error shows", |_, texts| {
        shows(texts, "The cache folder is not safe to purge.")
    });
    assert!(held.rig.dir.0.join("tarragon.db").exists());
    assert_eq!(held.rig.app.db.count_thumbnails().expect("count"), 3);
}

#[test]
fn the_window_stays_drawable_and_shows_the_strip_while_the_purge_waits_for_a_render() {
    let mut held = Held::new(false);
    held.open_held();
    held.open_window();
    held.rig.click("Purge Cache", false);
    held.until("the purge runs", |rig, _| {
        rig.app.thumbs.is_working() == Some(Work::Purge)
    });
    for _ in 0..10 {
        let start = Instant::now();
        let texts = held.rig.frame(Vec::new()).0;
        assert!(start.elapsed() < Duration::from_secs(2), "a frame blocked");
        assert!(shows(&texts, "Purging cache..."), "{texts:?}");
        assert!(shows(&texts, "Preferences"), "{texts:?}");
        assert!(lock(&shared(&held.rig.ctx)).purging);
    }
    held.release();
    held.until_purged();
    assert!(!shows(&held.rig.frame(Vec::new()).0, "Purging cache..."));
}

#[test]
fn an_eviction_result_is_taken_and_leaves_the_purge_state_alone() {
    let mut rig = Rig::new(&[], &[], 9);
    rig.frame(Vec::new());
    rig.app.thumbs.evict(0);
    let end = Instant::now() + LIMIT;
    while !rig.app.thumbs.is_idle() {
        rig.frame(Vec::new());
        assert!(Instant::now() < end, "the eviction never ended");
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(rig.app.thumbs.take_work_result().is_none(), "left unread");
    assert!(!lock(&shared(&rig.ctx)).purging);
}

#[test]
fn a_failed_save_reopens_the_window_with_the_measured_size_and_asks_for_frames() {
    let mut rig = Rig::new(&[], &[], 9);
    fill_cache(&rig.app.cache_root, &[1000, 2000, 3000]);
    open_from_menu(&mut rig);
    let want = size_text(6000);
    let end = Instant::now() + LIMIT;
    while !shows(&rig.frame(Vec::new()).0, &want) {
        assert!(Instant::now() < end, "the size {want} never showed");
        std::thread::sleep(Duration::from_millis(10));
    }
    // Let the settle counter run out, so the reopen has to set it again.
    for _ in 0..8 {
        rig.frame(Vec::new());
    }
    let bad = PrefsValues {
        tile_grid: "9x9".to_owned(),
        ..prefs::load(&rig.app.db)
    };
    rig.app.shell.prefs_mut().open(bad, "stale".to_owned());
    for _ in 0..4 {
        rig.frame(Vec::new());
    }
    rig.click("OK", false);
    let left = rig.ctx.data(|d| d.get_temp::<u8>(Id::new(SETTLE_ID)));
    assert!(left.is_some_and(|n| n > 0), "{left:?}");
    let texts = rig.frame(Vec::new()).0;
    assert!(shows(&texts, "tile_grid_size"), "{texts:?}");
    assert!(shows(&texts, &want), "{texts:?}");
    assert!(!shows(&texts, "stale"), "{texts:?}");
}

#[test]
fn reopening_the_window_during_a_purge_shows_the_strip() {
    let mut held = Held::new(false);
    held.open_held();
    held.open_window();
    held.rig.click("Purge Cache", false);
    held.until("the purge runs", |rig, _| {
        rig.app.thumbs.is_working() == Some(Work::Purge)
    });
    // A window that opens fresh does not know about the purge by itself.
    held.rig.app.shell.prefs_mut().set_purging(false);
    let ctx = held.rig.ctx.clone();
    held.rig.app.open_prefs(&ctx);
    held.until("the strip shows", |_, texts| {
        shows(texts, "Purging cache...")
    });
    held.release();
    held.until_purged();
}
