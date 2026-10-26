//! Headless tests for the pipeline. They use real fixtures, a real cache
//! folder, and a real database in a temp folder.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use image::{DynamicImage, Rgba, RgbaImage};
use tarragon_cache::store::CacheStore;
use tarragon_db::Database;
use tarragon_db::repository::ThumbnailWrite;

use super::{Config, RESULT_BOUND, RenderFn};
use super::{ErrorKind, Pipeline, Results, ThumbResult, new_folder_id};

/// How long a test waits for one result before it fails.
const WAIT: Duration = Duration::from_secs(60);

/// A temp folder that removes itself on drop.
struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "tarragon-pipeline-test-{}-{id}",
            std::process::id()
        ));
        std::fs::create_dir_all(&path).expect("make the temp folder");
        Self(path)
    }

    fn sub(&self, name: &str) -> PathBuf {
        let path = self.0.join(name);
        std::fs::create_dir_all(&path).expect("make a sub folder");
        path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn fixture(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures")
        .join(relative)
}

/// The low byte of a number.
fn byte(value: u32) -> u8 {
    u8::try_from(value % 256).unwrap_or(0)
}

/// Write a PNG whose pixels all have `alpha`.
fn write_png(path: &Path, alpha: u8) {
    let image = RgbaImage::from_fn(40, 30, |x, y| Rgba([byte(x * 6), byte(y * 8), 90, alpha]));
    image.save(path).expect("write a png");
}

/// A render function that counts calls per path and then calls the registry.
fn counting_render(calls: &Arc<Mutex<Vec<PathBuf>>>) -> RenderFn {
    let calls = Arc::clone(calls);
    Arc::new(move |path| {
        calls.lock().expect("calls lock").push(path.to_path_buf());
        tarragon_render::registry::render(path)
    })
}

struct Setup {
    // Declared first so the pipeline stops before the folder goes.
    pipeline: Pipeline,
    results: Results,
    db: Arc<Database>,
    cache_root: PathBuf,
    repaints: Arc<AtomicU64>,
    dir: TempDir,
}

fn start(dir: TempDir, workers: usize, render: RenderFn) -> Setup {
    let cache_root = dir.sub("cache");
    let db = Arc::new(Database::open(dir.0.join("test.db")).expect("open the db"));
    db.initialize().expect("initialize the db");
    resume(dir, db, cache_root, workers, render)
}

/// Start a pipeline on an existing database and cache.
fn resume(
    dir: TempDir,
    db: Arc<Database>,
    cache_root: PathBuf,
    workers: usize,
    render: RenderFn,
) -> Setup {
    let repaints = Arc::new(AtomicU64::new(0));
    let counter = Arc::clone(&repaints);
    let config = Config {
        cache: CacheStore::new(&cache_root),
        workers,
        render,
    };
    let (pipeline, results) = Pipeline::new(
        config,
        Arc::clone(&db),
        Arc::new(move || {
            counter.fetch_add(1, Ordering::Relaxed);
        }),
    )
    .expect("start the pipeline");
    Setup {
        pipeline,
        results,
        db,
        cache_root,
        repaints,
        dir,
    }
}

fn collect(results: &Results, count: usize) -> Vec<ThumbResult> {
    (0..count)
        .map(|_| results.recv_timeout(WAIT).expect("a result in time"))
        .collect()
}

fn name_of(result: &ThumbResult) -> String {
    result
        .key
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_owned()
}

/// A folder with every supported format and an alpha PNG.
fn mixed_folder(dir: &TempDir) -> PathBuf {
    let folder = dir.sub("mixed");
    write_png(&folder.join("plain.png"), 255);
    write_png(&folder.join("alpha.png"), 128);
    // No JPEG fixture is committed, so the test encodes a small one.
    let photo = RgbaImage::from_fn(48, 32, |x, y| Rgba([byte(x * 5), byte(y * 7), 40, 255]));
    DynamicImage::ImageRgba8(photo)
        .to_rgb8()
        .save(folder.join("photo.jpg"))
        .expect("write a jpeg");
    for (from, to) in [
        ("psd/psb_8bit.psb", "big.psb"),
        ("psd/rgb_8bit.psd", "doc.psd"),
        ("clip/preview.clip", "art.clip"),
        ("kra/merged.kra", "paint.kra"),
    ] {
        std::fs::copy(fixture(from), folder.join(to)).expect("copy a fixture");
    }
    folder
}

#[test]
fn every_format_gives_an_ok_result_and_alpha_caches_png() {
    let dir = TempDir::new();
    let folder = mixed_folder(&dir);
    let calls = Arc::default();
    let mut setup = start(dir, 2, counting_render(&calls));
    let generation = setup.pipeline.open_folder(&folder).expect("open");

    let results = collect(&setup.results, 7);
    let mut names: Vec<String> = results.iter().map(name_of).collect();
    names.sort();
    assert_eq!(
        names,
        [
            "alpha.png",
            "art.clip",
            "big.psb",
            "doc.psd",
            "paint.kra",
            "photo.jpg",
            "plain.png",
        ]
    );
    for result in &results {
        assert_eq!(result.generation, generation);
        let thumb = result.outcome.as_ref().expect("an ok result");
        assert!(thumb.width().max(thumb.height()) <= 256);
    }
    assert!(setup.repaints.load(Ordering::Relaxed) >= 7);

    setup.pipeline.flush();
    let uuid = setup
        .db
        .folder_uuid(&folder.to_string_lossy())
        .expect("read the uuid")
        .expect("a uuid");
    for tier in ["256", "1024", "full"] {
        let tier_file = setup
            .cache_root
            .join(tier)
            .join(format!("mixed_{uuid}"))
            .join("alpha.png.png");
        let bytes = std::fs::read(&tier_file).expect("a PNG tier file");
        assert!(bytes.starts_with(b"\x89PNG"), "{tier} is a PNG");
    }
    let row = setup
        .db
        .thumbnail(&folder.join("doc.psd").to_string_lossy())
        .expect("read a row")
        .expect("a row");
    assert!(row.thumbnail_cache_path.is_some());
    assert_eq!(row.cache_uuid.as_deref(), Some(uuid.as_str()));
}

#[test]
fn second_open_uses_the_cache_and_a_corrupt_tier_renders_again() {
    let dir = TempDir::new();
    let folder = mixed_folder(&dir);
    let calls: Arc<Mutex<Vec<PathBuf>>> = Arc::default();
    let mut setup = start(dir, 2, counting_render(&calls));
    setup.pipeline.open_folder(&folder).expect("open");
    collect(&setup.results, 7);
    setup.pipeline.flush();
    assert_eq!(calls.lock().expect("lock").len(), 7);

    setup.pipeline.open_folder(&folder).expect("open again");
    let again = collect(&setup.results, 7);
    assert!(again.iter().all(|result| result.outcome.is_ok()));
    assert_eq!(calls.lock().expect("lock").len(), 7, "no render on a hit");

    // A non-empty file that does not decode is a miss.
    let uuid = setup
        .db
        .folder_uuid(&folder.to_string_lossy())
        .expect("read the uuid")
        .expect("a uuid");
    let tier_file = setup
        .cache_root
        .join("256")
        .join(format!("mixed_{uuid}"))
        .join("plain.png.jpg");
    assert!(tier_file.is_file(), "an opaque png caches as jpeg");
    std::fs::write(&tier_file, b"not an image").expect("corrupt the tier");

    setup
        .pipeline
        .open_folder(&folder)
        .expect("open a third time");
    let third = collect(&setup.results, 7);
    assert!(third.iter().all(|result| result.outcome.is_ok()));
    let calls = calls.lock().expect("lock");
    assert_eq!(calls.len(), 8);
    assert_eq!(calls[7], folder.join("plain.png"));
}

#[test]
fn a_changed_source_renders_again() {
    let dir = TempDir::new();
    let folder = dir.sub("changing");
    let file = folder.join("one.png");
    write_png(&file, 255);
    let calls: Arc<Mutex<Vec<PathBuf>>> = Arc::default();
    let mut setup = start(dir, 1, counting_render(&calls));
    setup.pipeline.open_folder(&folder).expect("open");
    collect(&setup.results, 1);
    setup.pipeline.flush();

    // A new size changes the stamp even when the mtime second is the same.
    let bigger = RgbaImage::from_fn(80, 60, |x, y| Rgba([byte(x), byte(y), 7, 255]));
    bigger.save(&file).expect("rewrite the png");
    setup.pipeline.open_folder(&folder).expect("open again");
    collect(&setup.results, 1);
    assert_eq!(calls.lock().expect("lock").len(), 2);
}

#[test]
fn a_bad_file_gives_an_error_and_the_pool_keeps_going() {
    let dir = TempDir::new();
    let folder = dir.sub("broken");
    std::fs::copy(fixture("psd/truncated.psd"), folder.join("bad.psd")).expect("copy");
    for index in 0..4 {
        write_png(&folder.join(format!("ok{index}.png")), 255);
    }
    let calls: Arc<Mutex<Vec<PathBuf>>> = Arc::default();
    let mut setup = start(dir, 1, counting_render(&calls));
    setup.pipeline.open_folder(&folder).expect("open");

    let results = collect(&setup.results, 5);
    let bad = results
        .iter()
        .find(|result| name_of(result) == "bad.psd")
        .expect("a result for the bad file");
    let error = bad.outcome.as_ref().expect_err("the bad file fails");
    assert_eq!(error.kind, ErrorKind::Decode);
    assert_eq!(results.iter().filter(|r| r.outcome.is_ok()).count(), 4);

    // The same stamp is not tried again.
    setup.pipeline.open_folder(&folder).expect("open again");
    let again = collect(&setup.results, 5);
    let still = again
        .iter()
        .find(|result| name_of(result) == "bad.psd")
        .expect("a result for the bad file");
    assert_eq!(
        still.outcome.as_ref().expect_err("still an error").kind,
        ErrorKind::Decode
    );
    let bad_renders = calls
        .lock()
        .expect("lock")
        .iter()
        .filter(|path| path.ends_with("bad.psd"))
        .count();
    assert_eq!(bad_renders, 1);
}

#[test]
fn visible_items_render_first() {
    let dir = TempDir::new();
    let folder = dir.sub("grid");
    for index in 0..5 {
        write_png(&folder.join(format!("a{index}.png")), 255);
    }
    let (entered_tx, entered_rx) = crossbeam_channel::unbounded::<()>();
    let (release_tx, release_rx) = crossbeam_channel::unbounded::<()>();
    let first = Arc::new(std::sync::Once::new());
    let render: RenderFn = Arc::new(move |_| {
        // The first render waits, so the test can reorder the rest.
        first.call_once(|| {
            entered_tx.send(()).expect("signal");
            release_rx.recv().expect("release");
        });
        Ok(DynamicImage::ImageRgba8(RgbaImage::new(4, 4)))
    });
    let mut setup = start(dir, 1, render);
    setup.pipeline.open_folder(&folder).expect("open");
    entered_rx
        .recv_timeout(WAIT)
        .expect("the first job started");

    let visible: HashSet<PathBuf> = ["a4.png", "a3.png"]
        .iter()
        .map(|name| folder.join(name))
        .collect();
    setup.pipeline.reprioritise(&visible);
    release_tx.send(()).expect("release");

    let order: Vec<String> = collect(&setup.results, 5).iter().map(name_of).collect();
    assert_eq!(order, ["a0.png", "a3.png", "a4.png", "a1.png", "a2.png"]);
}

#[test]
fn an_old_generation_never_reaches_the_consumer() {
    let dir = TempDir::new();
    let first = dir.sub("first");
    let second = dir.sub("second");
    for index in 0..3 {
        write_png(&first.join(format!("f{index}.png")), 255);
    }
    for index in 0..2 {
        write_png(&second.join(format!("s{index}.png")), 255);
    }
    let (entered_tx, entered_rx) = crossbeam_channel::unbounded::<()>();
    let (release_tx, release_rx) = crossbeam_channel::unbounded::<()>();
    let once = Arc::new(std::sync::Once::new());
    let render: RenderFn = Arc::new(move |_| {
        once.call_once(|| {
            entered_tx.send(()).expect("signal");
            release_rx.recv().expect("release");
        });
        Ok(DynamicImage::ImageRgba8(RgbaImage::new(4, 4)))
    });
    let mut setup = start(dir, 1, render);
    let old = setup.pipeline.open_folder(&first).expect("open first");
    entered_rx.recv_timeout(WAIT).expect("a render started");
    let new = setup.pipeline.open_folder(&second).expect("open second");
    assert_eq!(new, old + 1);
    release_tx.send(()).expect("release");

    let results = collect(&setup.results, 2);
    assert!(results.iter().all(|r| r.generation == new));
    assert!(results.iter().all(|r| r.key.starts_with(&second)));
    assert!(
        setup
            .results
            .recv_timeout(Duration::from_millis(300))
            .is_none(),
        "nothing from the first folder arrives"
    );
}

#[test]
fn the_consumer_drops_a_result_of_an_old_generation() {
    let (tx, rx) = crossbeam_channel::unbounded();
    let results = Results {
        receiver: rx,
        generation: Arc::new(AtomicU64::new(2)),
    };
    let make = |generation, name: &str| ThumbResult {
        generation,
        key: PathBuf::from(name),
        outcome: Ok(RgbaImage::new(1, 1)),
    };
    tx.send(make(1, "old")).expect("send");
    tx.send(make(2, "new")).expect("send");
    assert_eq!(results.try_recv().expect("a result").key, Path::new("new"));
    assert!(results.try_recv().is_none());
}

#[test]
fn folder_ids_are_unique() {
    let ids: HashSet<String> = (0..1000).map(|_| new_folder_id()).collect();
    assert_eq!(ids.len(), 1000);
}

#[test]
fn a_reopened_database_keeps_rows_between_pipelines() {
    let dir = TempDir::new();
    let folder = dir.sub("again");
    write_png(&folder.join("one.png"), 255);
    let calls: Arc<Mutex<Vec<PathBuf>>> = Arc::default();
    let mut setup = start(dir, 1, counting_render(&calls));
    setup.pipeline.open_folder(&folder).expect("open");
    collect(&setup.results, 1);

    // Dropping the pipeline stores the queued rows.
    let Setup {
        pipeline,
        results,
        db,
        cache_root,
        dir,
        ..
    } = setup;
    drop((pipeline, results));
    let mut next = resume(dir, db, cache_root, 1, counting_render(&calls));
    next.pipeline.open_folder(&folder).expect("open");
    collect(&next.results, 1);
    assert_eq!(calls.lock().expect("lock").len(), 1);
}

#[test]
fn a_cache_hit_refreshes_last_accessed() {
    let dir = TempDir::new();
    let folder = dir.sub("touch");
    let file = folder.join("one.png");
    write_png(&file, 255);
    let mut setup = start(dir, 1, Arc::new(tarragon_render::registry::render));
    setup.pipeline.open_folder(&folder).expect("open");
    collect(&setup.results, 1);
    setup.pipeline.flush();

    // Set the access time back, so a refresh is easy to see.
    let text = file.to_string_lossy().into_owned();
    let row = setup.db.thumbnail(&text).expect("read").expect("a row");
    setup
        .db
        .upsert_thumbnail(&ThumbnailWrite {
            path: row.path.clone(),
            mtime: row.mtime,
            size: row.size,
            width: row.width,
            height: row.height,
            cache_uuid: row.cache_uuid.clone().expect("a uuid"),
            thumbnail_cache_path: row.thumbnail_cache_path.clone(),
            preview_cache_path: row.preview_cache_path.clone(),
            full_cache_path: row.full_cache_path.clone(),
            last_accessed: 1,
        })
        .expect("write the row");

    setup.pipeline.open_folder(&folder).expect("open again");
    collect(&setup.results, 1);
    setup.pipeline.flush();
    let after = setup.db.thumbnail(&text).expect("read").expect("a row");
    assert!(after.last_accessed > 1);
}

#[test]
fn dropping_the_pipeline_returns_when_nobody_reads_results() {
    let dir = TempDir::new();
    let folder = dir.sub("flood");
    for index in 0..(RESULT_BOUND + 40) {
        write_png(&folder.join(format!("f{index:03}.png")), 255);
    }
    let render: RenderFn = Arc::new(|_| Ok(DynamicImage::ImageRgba8(RgbaImage::new(4, 4))));
    let mut setup = start(dir, 2, render);
    setup.pipeline.open_folder(&folder).expect("open");
    // Wait until the channel is full, so a worker is stuck on a send.
    let begin = std::time::Instant::now();
    while setup.results.receiver.len() < RESULT_BOUND {
        assert!(begin.elapsed() < WAIT, "the channel did not fill");
        std::thread::sleep(Duration::from_millis(5));
    }
    let Setup {
        pipeline,
        results,
        dir,
        ..
    } = setup;
    let (done_tx, done_rx) = crossbeam_channel::bounded(1);
    std::thread::spawn(move || {
        drop(pipeline);
        let _ = done_tx.send(());
    });
    done_rx.recv_timeout(WAIT).expect("the drop returned");
    // The consumer lived the whole time.
    drop((results, dir));
}

#[test]
fn drain_returns_at_most_max_current_results() {
    let (tx, rx) = crossbeam_channel::unbounded();
    let results = Results {
        receiver: rx,
        generation: Arc::new(AtomicU64::new(1)),
    };
    for index in 0..5 {
        tx.send(ThumbResult {
            generation: u64::from(index != 0),
            key: PathBuf::from(format!("{index}")),
            outcome: Ok(RgbaImage::new(1, 1)),
        })
        .expect("send");
    }
    assert_eq!(results.drain(3).len(), 3);
    assert_eq!(results.drain(10).len(), 1);
}

#[test]
fn a_changed_stamp_clears_a_remembered_failure() {
    let dir = TempDir::new();
    let folder = dir.sub("fixed");
    let file = folder.join("doc.psd");
    std::fs::copy(fixture("psd/truncated.psd"), &file).expect("copy");
    let mut setup = start(dir, 1, Arc::new(tarragon_render::registry::render));
    setup.pipeline.open_folder(&folder).expect("open");
    assert!(collect(&setup.results, 1)[0].outcome.is_err());

    std::fs::copy(fixture("psd/rgb_8bit.psd"), &file).expect("replace");
    setup.pipeline.open_folder(&folder).expect("open again");
    assert!(collect(&setup.results, 1)[0].outcome.is_ok());
}

#[test]
fn only_permanent_failures_are_remembered() {
    assert!(ErrorKind::Decode.is_permanent());
    assert!(ErrorKind::UnsupportedFormat.is_permanent());
    assert!(ErrorKind::TooLarge.is_permanent());
    assert!(ErrorKind::InvalidName.is_permanent());
    for kind in [
        ErrorKind::Io,
        ErrorKind::Timeout,
        ErrorKind::Cancelled,
        ErrorKind::Panicked,
    ] {
        assert!(!kind.is_permanent());
    }
}
