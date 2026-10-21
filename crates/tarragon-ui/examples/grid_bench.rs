//! Headless CPU frame-time benchmark for the virtualized grid and the gallery.
//!
//! The example drives a 10k-item grid through egui with no window. It measures
//! the CPU cost of one frame:
//!
//! 1. `Context::run_ui` - widget layout and paint-list build.
//! 2. `Context::tessellate` - shapes to triangle meshes.
//!
//! It reports min, average, median, p95, and max frame time.
//!
//! Two passes run at each width:
//!
//! 1. The grid container only (`tarragon_ui::grid::VirtualGrid::show_items_at`).
//!    The pass sweeps the whole list, so the virtualization is visible.
//! 2. The whole gallery (`tarragon_ui::gallery::Gallery::show`), strip and grid
//!    together, at 10k items. The pass sends no input, so it measures a quiet
//!    frame that reads the cached visible order.
//!
//! The tile carries a simplified data set: a file name, a badge derived from
//! the path, and no texture. The measured frames cover two widths: the shell
//! grid width near 874 pt, and the wide 1600 pt viewport.
//!
//! The text path is the vendored Roboto family, installed through
//! `tarragon_ui::install`, so the run measures the shipped text path.
//!
//! The measured frame loop runs inside a rayon pool capped at four worker
//! threads.
//!
//! ```text
//! cargo run --release --example grid_bench -p tarragon-ui
//! ```
//!
//! The numeric casts are for a measurement harness, not for the library.

#![allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]

use std::path::PathBuf;
use std::time::{Duration, Instant};

use egui::{Context, RawInput, Rect, pos2, vec2};
use tarragon_core::sorting::SortMode;
use tarragon_ui::gallery::Gallery;
use tarragon_ui::grid::{GridItem, GridMetrics, Selection, VirtualGrid, metrics_for};

/// Number of items in the grid and in the gallery.
const ITEM_COUNT: usize = 10_000;
/// Number of measured frames.
const FRAMES: usize = 400;
/// Number of frames before the measurement. The first frame builds the fonts.
const WARMUP_FRAMES: usize = 40;
/// Grid widths in points: the shell grid width and the wide viewport.
const VIEW_WIDTHS: [f32; 2] = [874.0, 1600.0];
/// Viewport height in points.
const VIEW_HEIGHT: f32 = 900.0;
/// Thread count of the measurement pool. This is the four-thread cap.
const POOL_THREADS: usize = 4;

/// The numbers from one grid measurement run.
struct Report {
    width: f32,
    samples: Vec<Duration>,
    cells_min: usize,
    cells_max: usize,
    cells_total: usize,
    pixels_per_point: f32,
    metrics: GridMetrics,
}

/// The numbers from one gallery measurement run.
struct GalleryReport {
    width: f32,
    samples: Vec<Duration>,
    visible: usize,
    painted: usize,
    pixels_per_point: f32,
}

fn main() {
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(POOL_THREADS)
        .build()
        .expect("failed to build the measurement pool");
    let pool_threads = pool.current_num_threads();

    println!("== tarragon grid bench ==");
    println!("items:            {ITEM_COUNT}");
    println!(
        "frame cost:       grid container, then the whole gallery; simplified data (name, badge, no texture)"
    );
    println!("text path:        vendored Roboto (Regular, Medium, Bold)");
    println!("rayon pool:       {pool_threads} threads (measurement cap)");
    println!("cpu affinity:     {}", cpu_affinity());
    println!("display/GPU:      none (headless; egui layout + tessellation only)");

    for width in VIEW_WIDTHS {
        let report = pool.install(|| measure(width));
        print_report(&report);
    }

    for width in VIEW_WIDTHS {
        let report = pool.install(|| measure_gallery(width));
        print_gallery_report(&report);
    }
}

/// Print one grid measurement run.
fn print_report(report: &Report) {
    let samples = &report.samples;
    let average_cells = report.cells_total / samples.len();
    let median = samples[samples.len() / 2];
    let p95 = samples[samples.len() * 95 / 100];
    let average = samples.iter().map(Duration::as_secs_f64).sum::<f64>() / samples.len() as f64;

    println!();
    println!("-- grid, viewport {}x{VIEW_HEIGHT} pt --", report.width);
    println!(
        "grid:             {} columns, cell {:.1}x{:.1} pt, spacing {:.1} pt",
        report.metrics.columns,
        report.metrics.cell.x,
        report.metrics.cell.y,
        report.metrics.spacing
    );
    println!("pixels per point: {}", report.pixels_per_point);
    println!(
        "frames measured:  {} (warmup {WARMUP_FRAMES})",
        samples.len()
    );
    println!(
        "cells built/frame: min {}, max {}, avg {average_cells}",
        report.cells_min, report.cells_max
    );
    println!(
        "frame cpu ms:     min {:.3}, avg {:.3}, median {:.3}, p95 {:.3}, max {:.3}",
        millis(samples[0]),
        average * 1000.0,
        millis(median),
        millis(p95),
        millis(samples[samples.len() - 1])
    );
}

/// Print one gallery measurement run.
fn print_gallery_report(report: &GalleryReport) {
    let samples = &report.samples;
    let median = samples[samples.len() / 2];
    let p95 = samples[samples.len() * 95 / 100];
    let average = samples.iter().map(Duration::as_secs_f64).sum::<f64>() / samples.len() as f64;

    println!();
    println!("-- gallery, viewport {}x{VIEW_HEIGHT} pt --", report.width);
    println!("visible items:    {}", report.visible);
    println!("tiles painted:    {}", report.painted);
    println!("pixels per point: {}", report.pixels_per_point);
    println!(
        "frames measured:  {} (warmup {WARMUP_FRAMES})",
        samples.len()
    );
    println!(
        "frame cpu ms:     min {:.3}, avg {:.3}, median {:.3}, p95 {:.3}, max {:.3}",
        millis(samples[0]),
        average * 1000.0,
        millis(median),
        millis(p95),
        millis(samples[samples.len() - 1])
    );
}

/// The ten thousand items of one run.
fn items() -> Vec<GridItem> {
    (0..ITEM_COUNT)
        .map(|index| {
            GridItem::new(
                PathBuf::from("/images").join(format!("IMG_{index:05}.jpg")),
                1,
                None,
            )
        })
        .collect()
}

/// The headless input for one frame at a grid width.
fn input(width: f32, time: f64) -> RawInput {
    RawInput {
        screen_rect: Some(Rect::from_min_size(
            pos2(0.0, 0.0),
            vec2(width, VIEW_HEIGHT),
        )),
        time: Some(time),
        ..Default::default()
    }
}

/// Install the vendored fonts and apply no other theme, as the grid pass asks.
fn install_fonts(ctx: &Context) {
    tarragon_ui::install(ctx);
}

/// Run the frame loop at one grid width and collect the frame times.
fn measure(width: f32) -> Report {
    let ctx = Context::default();
    install_fonts(&ctx);
    let metrics = metrics_for(width);
    let items = items();
    let order: Vec<usize> = (0..items.len()).collect();
    let selection = Selection::default();
    let content = metrics.content_size(items.len());
    let max_scroll = (content.y - VIEW_HEIGHT).max(0.0);

    let mut samples: Vec<Duration> = Vec::with_capacity(FRAMES);
    let mut cells_min = usize::MAX;
    let mut cells_max = 0usize;
    let mut cells_total = 0usize;
    let mut pixels_per_point = 1.0f32;

    for frame in 0..(WARMUP_FRAMES + FRAMES) {
        // Sweep the whole list across the measured frames.
        let fraction = frame as f32 / (WARMUP_FRAMES + FRAMES) as f32;
        let scroll_y = max_scroll * fraction;
        let raw = input(width, frame as f64 / 60.0);

        let started = Instant::now();
        let grid = VirtualGrid {
            metrics,
            item_count: items.len(),
        };
        let mut cell_count = 0usize;
        let output = ctx.run_ui(raw, |ui| {
            cell_count = grid
                .show_items_at(ui, scroll_y, &items, &order, &selection)
                .painted;
        });
        let _primitives = ctx.tessellate(output.shapes, output.pixels_per_point);
        let elapsed = started.elapsed();
        pixels_per_point = output.pixels_per_point;

        if frame >= WARMUP_FRAMES {
            samples.push(elapsed);
            cells_min = cells_min.min(cell_count);
            cells_max = cells_max.max(cell_count);
            cells_total += cell_count;
        }
    }

    samples.sort_unstable();
    Report {
        width,
        samples,
        cells_min,
        cells_max,
        cells_total,
        pixels_per_point,
        metrics,
    }
}

/// Run the frame loop of the whole gallery at one width and collect the times.
fn measure_gallery(width: f32) -> GalleryReport {
    let ctx = Context::default();
    install_fonts(&ctx);
    let mut gallery = Gallery::new(SortMode::Natural);
    gallery.set_folder(Some(PathBuf::from("/images")));
    gallery.set_items(items());

    let mut samples: Vec<Duration> = Vec::with_capacity(FRAMES);
    let mut pixels_per_point = 1.0f32;

    for frame in 0..(WARMUP_FRAMES + FRAMES) {
        let raw = input(width, frame as f64 / 60.0);

        let started = Instant::now();
        let output = ctx.run_ui(raw, |ui| gallery.show(ui));
        let _primitives = ctx.tessellate(output.shapes, output.pixels_per_point);
        let elapsed = started.elapsed();
        pixels_per_point = output.pixels_per_point;

        if frame >= WARMUP_FRAMES {
            samples.push(elapsed);
        }
    }

    samples.sort_unstable();
    GalleryReport {
        width,
        samples,
        visible: gallery.visible_len(),
        painted: painted_tiles(&ctx),
        pixels_per_point,
    }
}

/// The number of tiles that the last gallery frame painted.
///
/// The natural order over `IMG_00000` and the folder scope put item index `n`
/// at display position `n`, so the painted ids run from zero without a gap.
fn painted_tiles(ctx: &Context) -> usize {
    (0..ITEM_COUNT)
        .take_while(|index| {
            ctx.read_response(egui::Id::new(("tarragon-grid-tile", *index)))
                .is_some()
        })
        .count()
}

/// Convert a duration to milliseconds.
fn millis(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

/// Read the CPU list that the process may run on.
fn cpu_affinity() -> String {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|status| {
            status.lines().find_map(|line| {
                line.strip_prefix("Cpus_allowed_list:")
                    .map(|list| list.trim().to_owned())
            })
        })
        .unwrap_or_else(|| "unknown".to_owned())
}
