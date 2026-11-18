//! A measurement run for a library of 100,000 rows. It is slow, so it is
//! ignored. Run it in release mode and read the printed numbers:
//!
//! ```text
//! cargo nextest run --release -p tarragon-app --run-ignored only --no-capture library_of_100k
//! ```
//!
//! The run is headless: egui lays out and tessellates, and no window opens.

#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_wrap,
    clippy::print_stdout
)]

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use eframe::egui::{
    Context, Event, Modifiers, MouseWheelUnit, RawInput, Rect, TouchPhase, pos2, vec2,
};
use image::{DynamicImage, RgbaImage};
use tarragon_cache::store::CacheStore;
use tarragon_core::entity::FileInfo;
use tarragon_core::sorting::SortMode;
use tarragon_db::Database;
use tarragon_ui::gallery::Scope;

use super::tests::TempDir;
use super::*;
use crate::pipeline::Config;

/// Rows in the stored library.
const ROWS: usize = 100_000;
/// Rows in each folder.
const ROWS_PER_FOLDER: usize = 100;
/// Bar: both database reads, on the open thread, in seconds.
const OPEN_BAR: f64 = 1.0;
/// Bar: the load into the gallery, on the UI thread, in milliseconds.
const LOAD_BAR_MS: f64 = 250.0;
/// Bar: one frame, in milliseconds.
const FRAME_BAR_MS: f64 = 100.0;
/// Bar: memory that the library adds, in MiB.
const RSS_BAR_MIB: f64 = 150.0;
/// Viewport in points.
const VIEW: (f32, f32) = (1280.0, 800.0);
/// The scroll step of each frame, in points.
const SCROLL_STEP: f32 = 700.0;
/// The longest the scroll phase may run.
const SCROLL_LIMIT: Duration = Duration::from_secs(90);

/// Read a value of `/proc/self/status` in MiB. A missing value fails the test.
fn status_mib(key: &str) -> f64 {
    let status = std::fs::read_to_string("/proc/self/status").expect("read /proc/self/status");
    let kib = status
        .lines()
        .find_map(|line| line.strip_prefix(key))
        .and_then(|rest| {
            rest.trim()
                .trim_end_matches("kB")
                .trim()
                .parse::<f64>()
                .ok()
        })
        .unwrap_or_else(|| panic!("no {key} in /proc/self/status"));
    kib / 1024.0
}

/// Make the peak counter equal to the current size. A failure fails the test.
fn reset_peak() {
    std::fs::write("/proc/self/clear_refs", "5").expect("reset the peak memory counter");
}

/// The bytes of all textures that the context holds.
fn texture_bytes(ctx: &Context) -> usize {
    ctx.tex_manager()
        .read()
        .allocated()
        .map(|(_, meta)| meta.bytes_used())
        .sum()
}

fn millis(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

/// Store `ROWS` rows in folders of `ROWS_PER_FOLDER`. A scan also stores each
/// folder, so the seed does the same. The first `unstored` folders get no
/// folder row.
fn seed(db: &Database, unstored: usize) {
    for folder in 0..ROWS / ROWS_PER_FOLDER {
        let parent = format!("/library/set{folder:04}");
        if folder >= unstored {
            db.get_or_create_folder_uuid(&parent, &format!("seed-{folder}"))
                .expect("store the folder");
        }
        let files: Vec<FileInfo> = (0..ROWS_PER_FOLDER)
            .map(|file| FileInfo {
                path: PathBuf::from(format!("/library/set{folder:04}/IMG_{file:04}.png")),
                size: 1000 + file as u64,
                mtime: 1_700_000_000 + folder as i64,
                width: 0,
                height: 0,
            })
            .collect();
        db.write_scan_group(&files).expect("seed a folder");
    }
}

/// The input of one frame, with a wheel step over the grid.
fn frame_input(time: f64, scroll: f32) -> RawInput {
    let mut input = RawInput {
        screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(VIEW.0, VIEW.1))),
        time: Some(time),
        ..Default::default()
    };
    input
        .events
        .push(Event::PointerMoved(pos2(VIEW.0 / 2.0, VIEW.1 / 2.0)));
    if scroll != 0.0 {
        input.events.push(Event::MouseWheel {
            unit: MouseWheelUnit::Point,
            delta: vec2(0.0, -scroll),
            phase: TouchPhase::Move,
            modifiers: Modifiers::NONE,
        });
    }
    input
}

/// Run one frame the way the app does, and return its time.
fn frame(ctx: &Context, thumbs: &mut Thumbs, gallery: &mut Gallery, input: RawInput) -> Duration {
    let started = Instant::now();
    thumbs.before_draw(ctx, gallery);
    let output = ctx.run_ui(input, |ui| gallery.show(ui));
    thumbs.after_draw(gallery);
    let _primitives = ctx.tessellate(output.shapes, output.pixels_per_point);
    started.elapsed()
}

/// Scroll down the library while the workers render. Return the frame times
/// and the peak bytes of the grid textures.
fn scroll(ctx: &Context, thumbs: &mut Thumbs, gallery: &mut Gallery) -> (Vec<Duration>, usize) {
    let mut samples = Vec::new();
    // The font atlas is not a grid texture, so the count starts after it.
    let font_bytes = texture_bytes(ctx);
    let mut peak_texture = 0usize;
    let started = Instant::now();
    let mut index = 0u32;
    while started.elapsed() < SCROLL_LIMIT {
        index += 1;
        let input = frame_input(1.0 + f64::from(index) / 60.0, SCROLL_STEP);
        samples.push(frame(ctx, thumbs, gallery, input));
        peak_texture = peak_texture.max(texture_bytes(ctx).saturating_sub(font_bytes));
        if index >= 300 && peak_texture >= GRID_BUDGET_BYTES / 10 * 9 {
            break;
        }
        std::thread::sleep(Duration::from_millis(8));
    }
    (samples, peak_texture)
}

/// What one run measured.
struct Numbers {
    files_time: Duration,
    open_time: Duration,
    load_time: Duration,
    first_frame: Duration,
    /// Frame times, sorted.
    samples: Vec<Duration>,
    rss_added: f64,
    rss_scrolled: f64,
    peak_texture: usize,
}

impl Numbers {
    /// Both database reads, in seconds.
    fn open_secs(&self) -> f64 {
        (self.files_time + self.open_time).as_secs_f64()
    }

    /// The slowest frame.
    fn max(&self) -> Duration {
        *self.samples.last().expect("a frame ran")
    }
}

/// Print the numbers with a verdict for each bar.
fn report(n: &Numbers) {
    let verdict = |pass: bool| if pass { "pass" } else { "FAIL" };
    let median = n.samples[n.samples.len() / 2];
    let p95 = n.samples[n.samples.len() * 95 / 100];
    println!("== 100k library ==");
    println!(
        "rows:            {ROWS} in {} folders",
        ROWS / ROWS_PER_FOLDER
    );
    println!(
        "library_files:   {:.1} ms; open_library: {:.1} ms (it reads the rows again); sum {:.3} s, bar {OPEN_BAR} s: {}",
        millis(n.files_time),
        millis(n.open_time),
        n.open_secs(),
        verdict(n.open_secs() < OPEN_BAR)
    );
    println!(
        "finish_library:  {:.1} ms (set_items, sort, first refresh_view), bar {LOAD_BAR_MS} ms: {}",
        millis(n.load_time),
        verdict(millis(n.load_time) < LOAD_BAR_MS)
    );
    println!(
        "frames:          {} (first {:.2} ms), median {:.2} ms, p95 {:.2} ms, max {:.2} ms, bar {FRAME_BAR_MS} ms: {}",
        n.samples.len(),
        millis(n.first_frame),
        millis(median),
        millis(p95),
        millis(n.max()),
        verdict(millis(n.max()) < FRAME_BAR_MS)
    );
    println!(
        "rss added:       {:.1} MiB after the first frame, {:.1} MiB after the scroll, bar {RSS_BAR_MIB} MiB (checked after the scroll): {}",
        n.rss_added,
        n.rss_scrolled,
        verdict(n.rss_scrolled < RSS_BAR_MIB)
    );
    println!(
        "textures:        peak {:.1} MiB of {} MiB: {}",
        n.peak_texture as f64 / 1_048_576.0,
        GRID_BUDGET_BYTES / 1_048_576,
        verdict(n.peak_texture <= GRID_BUDGET_BYTES)
    );
}

#[test]
#[ignore = "measurement run: needs a release build and about a minute"]
fn library_of_100k_meets_the_bars() {
    let dir = TempDir(std::env::temp_dir().join(format!("tarragon-scale-{}", std::process::id())));
    std::fs::create_dir_all(&dir.0).expect("make the temp dir");
    let db = Arc::new(Database::open(dir.0.join("t.db")).expect("open the db"));
    db.initialize().expect("initialize the db");
    seed(&db, 0);

    let config = Config {
        cache: CacheStore::new(dir.0.join("cache")),
        workers: 4,
        render: Arc::new(|_| Ok(DynamicImage::ImageRgba8(RgbaImage::new(256, 256)))),
    };
    let repaint: Repaint = Arc::new(|| {});
    let (mut pipeline, results) =
        Pipeline::new(config, Arc::clone(&db), Arc::clone(&repaint)).expect("start the pipeline");
    let ctx = Context::default();
    tarragon_ui::install(&ctx);
    let mut gallery = Gallery::new(SortMode::Natural);
    gallery.set_scope(Scope::All);

    // Warm the fonts, so the numbers do not hold the one-time setup.
    let _ = ctx.run_ui(frame_input(0.0, 0.0), |ui| gallery.show(ui));
    let _ = ctx.run_ui(frame_input(0.02, 0.0), |ui| gallery.show(ui));

    reset_peak();
    let rss_before = status_mib("VmRSS:");

    let started = Instant::now();
    let files = db.library_files().expect("read the rows");
    let files_time = started.elapsed();
    assert_eq!(files.len(), ROWS);
    drop(files);

    let started = Instant::now();
    let opened = pipeline.open_library().expect("open the library");
    let open_time = started.elapsed();
    assert_eq!(opened.files.len(), ROWS);

    let mut thumbs = Thumbs::new(pipeline, results, None, Arc::clone(&repaint));
    let started = Instant::now();
    thumbs.finish_library(&mut gallery, Ok(opened));
    let load_time = started.elapsed();
    assert_eq!(gallery.visible_len(), ROWS);

    let first_frame = frame(&ctx, &mut thumbs, &mut gallery, frame_input(1.0, 0.0));
    let rss_added = status_mib("VmHWM:") - rss_before;

    let (mut samples, peak_texture) = scroll(&ctx, &mut thumbs, &mut gallery);
    samples.push(first_frame);
    let rss_scrolled = status_mib("VmHWM:") - rss_before;
    samples.sort_unstable();

    let numbers = Numbers {
        files_time,
        open_time,
        load_time,
        first_frame,
        samples,
        rss_added,
        rss_scrolled,
        peak_texture,
    };
    report(&numbers);

    assert!(numbers.open_secs() < OPEN_BAR, "the open bar failed");
    assert!(millis(load_time) < LOAD_BAR_MS, "the load bar failed");
    assert!(millis(numbers.max()) < FRAME_BAR_MS, "the frame bar failed");
    assert!(rss_scrolled < RSS_BAR_MIB, "the memory bar failed");
    assert!(
        peak_texture <= GRID_BUDGET_BYTES,
        "the texture budget broke"
    );
    assert!(
        peak_texture >= GRID_BUDGET_BYTES / 10 * 9,
        "the scroll never filled the budget, so the check proves nothing"
    );
}

/// Bar: `open_library` when every folder has no folder row, in seconds.
const MISSING_OPEN_BAR: f64 = 1.0;

#[test]
#[ignore = "measurement run: needs a release build"]
fn library_of_100k_without_folder_rows_opens_fast() {
    let dir =
        TempDir(std::env::temp_dir().join(format!("tarragon-missing-{}", std::process::id())));
    std::fs::create_dir_all(&dir.0).expect("make the temp dir");
    let db = Arc::new(Database::open(dir.0.join("t.db")).expect("open the db"));
    db.initialize().expect("initialize the db");
    seed(&db, ROWS / ROWS_PER_FOLDER);

    let config = Config {
        cache: CacheStore::new(dir.0.join("cache")),
        workers: 4,
        render: Arc::new(|_| Ok(DynamicImage::ImageRgba8(RgbaImage::new(256, 256)))),
    };
    let repaint: Repaint = Arc::new(|| {});
    let (mut pipeline, _results) =
        Pipeline::new(config, Arc::clone(&db), repaint).expect("start the pipeline");

    let started = Instant::now();
    let first = pipeline.open_library().expect("open the library");
    let first_time = started.elapsed();
    assert_eq!(first.files.len(), ROWS);

    println!("== 100k library, no folder rows ==");
    println!(
        "open_library:    {:.1} ms with {} missing folder rows; bar {MISSING_OPEN_BAR} s",
        millis(first_time),
        ROWS / ROWS_PER_FOLDER,
    );
    assert!(
        first_time.as_secs_f64() < MISSING_OPEN_BAR,
        "the missing-rows open bar failed"
    );
}
