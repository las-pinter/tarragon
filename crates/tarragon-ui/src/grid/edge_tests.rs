//! The scroll bar must not cover the right-hand tile column. Each test runs
//! headless: no window and no GPU.

use egui::{Context, Pos2, RawInput, Rect, Vec2, vec2};

use super::{VirtualGrid, metrics_for};
use crate::tokens;

/// Grid width in points.
const WIDTH: f32 = 1200.0;
/// Grid height in points when the items overflow.
const SHORT: f32 = 300.0;
/// Grid height in points when the items fit.
const TALL: f32 = 900.0;
/// Items that fill two full rows at this width.
const FEW: usize = 10;
/// Items that need many rows to overflow the short height.
const MANY: usize = 60;
/// Frames before the measured frame, so the bar animation and the repaints stop.
const SETTLE: usize = 8;
/// Point difference that counts as equal.
const EPSILON: f32 = 1e-3;
/// The display scales under test.
const SCALES: [f32; 3] = [1.0, 1.5, 2.0];

fn raw(scale: f32, size: Vec2) -> RawInput {
    let mut viewports = egui::ViewportIdMap::default();
    viewports.insert(
        egui::ViewportId::ROOT,
        egui::ViewportInfo {
            native_pixels_per_point: Some(scale),
            ..Default::default()
        },
    );
    RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
        viewports,
        ..Default::default()
    }
}

/// Largest right edge of any painted cell in the last frame, in points.
fn cells_right(scale: f32, item_count: usize, height: f32) -> f32 {
    let ctx = Context::default();
    crate::install(&ctx);
    crate::apply_theme(&ctx);
    let grid = VirtualGrid {
        metrics: metrics_for(WIDTH),
        item_count,
    };
    let mut right = 0.0_f32;
    for _ in 0..=SETTLE {
        right = 0.0;
        ctx.run_ui(raw(scale, vec2(WIDTH, height)), |ui| {
            grid.show_at(ui, 0.0, |_, rect, _| {
                right = right.max(rect.max.x);
            });
        })
        .drop_without_applying_deltas();
    }
    right
}

#[test]
fn an_overflowing_grid_leaves_room_for_the_bar_and_the_gap() {
    // The room comes from public values, not from the style under test, so a
    // wrong gap in the style fails too.
    let room = egui::style::ScrollStyle::solid().bar_width + tokens::SPACING_S;
    for scale in SCALES {
        let right = cells_right(scale, MANY, SHORT);
        assert!(
            (right - (WIDTH - room)).abs() < EPSILON,
            "the tiles end at {right}, not at {}, at scale {scale}",
            WIDTH - room
        );
    }
}

#[test]
fn a_grid_that_fits_uses_the_full_width() {
    for scale in SCALES {
        let right = cells_right(scale, FEW, TALL);
        assert!(
            (right - WIDTH).abs() < EPSILON,
            "the tiles end at {right}, not at {WIDTH}, at scale {scale}"
        );
    }
}
