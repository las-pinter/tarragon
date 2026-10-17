//! Headless CPU frame-time benchmark for the virtualized grid.
//!
//! The example drives a 10k-item grid through egui with no window. It measures
//! the CPU cost of one frame:
//!
//! 1. `Context::run_ui` - widget layout and paint-list build.
//! 2. `Context::tessellate` - shapes to triangle meshes.
//!
//! It reports min, median, p95, and max frame time. It also reports how many
//! cells each frame builds, so the virtualization is visible.
//!
//! The build keeps the egui default fonts off (their licenses are off the
//! allowlist). The example loads a system font at run time. It measures the
//! text path too when a system font is found.
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

use std::sync::Arc;
use std::time::{Duration, Instant};

use egui::{
    Align2, Color32, Context, FontData, FontDefinitions, FontFamily, FontId, RawInput, Rect, pos2,
    vec2,
};
use tarragon_ui::grid::{GridMetrics, VirtualGrid};

/// Number of items in the grid.
const ITEM_COUNT: usize = 10_000;
/// Number of measured frames.
const FRAMES: usize = 400;
/// Number of frames before the measurement. The first frame builds the fonts.
const WARMUP_FRAMES: usize = 40;
/// Viewport width in points.
const VIEW_WIDTH: f32 = 1600.0;
/// Viewport height in points.
const VIEW_HEIGHT: f32 = 900.0;
/// Grid column count.
const COLUMNS: usize = 6;
/// Thread count of the measurement pool. This is the four-thread cap.
const POOL_THREADS: usize = 4;
/// Cell colors. The value is fixed, so the run repeats.
const CELL_COLORS: [Color32; 4] = [
    Color32::from_rgb(58, 66, 82),
    Color32::from_rgb(74, 58, 66),
    Color32::from_rgb(58, 82, 70),
    Color32::from_rgb(82, 76, 58),
];

/// The numbers from one measurement run.
struct Report {
    samples: Vec<Duration>,
    cells_min: usize,
    cells_max: usize,
    cells_total: usize,
    pixels_per_point: f32,
    has_font: bool,
}

fn main() {
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(POOL_THREADS)
        .build()
        .expect("failed to build the measurement pool");
    let pool_threads = pool.current_num_threads();
    let report = pool.install(measure);

    let samples = &report.samples;
    let average_cells = report.cells_total / samples.len();
    let median = samples[samples.len() / 2];
    let p95 = samples[samples.len() * 95 / 100];
    let average = samples.iter().map(Duration::as_secs_f64).sum::<f64>() / samples.len() as f64;

    println!("== tarragon grid bench ==");
    println!("items:            {ITEM_COUNT}");
    println!("grid:             {COLUMNS} columns, cell 240x180 pt, spacing 8 pt");
    println!(
        "viewport:         {VIEW_WIDTH}x{VIEW_HEIGHT} pt, pixels_per_point {}",
        report.pixels_per_point
    );
    println!(
        "frames measured:  {} (warmup {WARMUP_FRAMES})",
        samples.len()
    );
    println!(
        "cells built/frame: min {}, max {}, avg {average_cells}",
        report.cells_min, report.cells_max
    );
    println!(
        "text path:        {}",
        if report.has_font {
            "system font"
        } else {
            "none"
        }
    );
    println!("rayon pool:       {pool_threads} threads (measurement cap)");
    println!("cpu affinity:     {}", cpu_affinity());
    println!("display/GPU:      none (headless; egui layout + tessellation only)");
    println!(
        "frame cpu ms:     min {:.3}, avg {:.3}, median {:.3}, p95 {:.3}, max {:.3}",
        millis(samples[0]),
        average * 1000.0,
        millis(median),
        millis(p95),
        millis(samples[samples.len() - 1])
    );
}

/// Run the frame loop and collect the frame times.
fn measure() -> Report {
    let ctx = Context::default();
    let has_font = install_system_font(&ctx);
    let metrics = GridMetrics {
        columns: COLUMNS,
        cell: vec2(240.0, 180.0),
        spacing: 8.0,
    };
    let content = metrics.content_size(ITEM_COUNT);
    let max_scroll = (content.y - VIEW_HEIGHT).max(0.0);
    let label_font = has_font.then(|| FontId::proportional(13.0));

    let mut samples: Vec<Duration> = Vec::with_capacity(FRAMES);
    let mut cells_min = usize::MAX;
    let mut cells_max = 0usize;
    let mut cells_total = 0usize;
    let mut pixels_per_point = 1.0f32;

    for frame in 0..(WARMUP_FRAMES + FRAMES) {
        // Sweep the whole list across the measured frames.
        let fraction = frame as f32 / (WARMUP_FRAMES + FRAMES) as f32;
        let scroll_y = max_scroll * fraction;

        let input = RawInput {
            screen_rect: Some(Rect::from_min_size(
                pos2(0.0, 0.0),
                vec2(VIEW_WIDTH, VIEW_HEIGHT),
            )),
            time: Some(frame as f64 / 60.0),
            ..Default::default()
        };

        let started = Instant::now();
        let mut cell_count = 0usize;
        let output = ctx.run_ui(input, |ui| {
            let grid = VirtualGrid {
                metrics,
                item_count: ITEM_COUNT,
            };
            cell_count = grid.show(ui, scroll_y, |ui, rect, index| {
                let color = CELL_COLORS[index % CELL_COLORS.len()];
                let painter = ui.painter();
                painter.rect_filled(rect, 4.0, color);
                if let Some(font) = &label_font {
                    painter.text(
                        rect.left_top() + vec2(8.0, 8.0),
                        Align2::LEFT_TOP,
                        format!("IMG_{index:05}.jpg"),
                        font.clone(),
                        Color32::WHITE,
                    );
                }
            });
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
        samples,
        cells_min,
        cells_max,
        cells_total,
        pixels_per_point,
        has_font,
    }
}

/// Convert a duration to milliseconds.
fn millis(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

/// Install a system font so the text path is measured.
///
/// The example keeps the egui default fonts off, because their licenses are off
/// the allowlist. The font is read at run time, so no font enters the binary.
fn install_system_font(ctx: &Context) -> bool {
    const CANDIDATES: [&str; 3] = [
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
        "/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf",
    ];

    for path in CANDIDATES {
        let Ok(bytes) = std::fs::read(path) else {
            continue;
        };
        let mut fonts = FontDefinitions::empty();
        fonts.font_data.insert(
            "grid-sans".to_owned(),
            Arc::new(FontData::from_owned(bytes)),
        );
        for family in [FontFamily::Proportional, FontFamily::Monospace] {
            fonts
                .families
                .entry(family)
                .or_default()
                .push("grid-sans".to_owned());
        }
        ctx.set_fonts(fonts);
        return true;
    }
    false
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
