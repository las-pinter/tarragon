//! Tests for the grid geometry and the grid scroll state.

use super::*;
use egui::{Context, Event, Id, MouseWheelUnit, PointerButton, Pos2, RawInput, TouchPhase};
use std::path::PathBuf;

/// Item count tall enough to scroll in a 700 point viewport.
const SCROLL_ITEMS: usize = 240;
/// Points one synthesized wheel step moves the content.
const WHEEL_STEP: f32 = 7.0;
/// Wheel steps that move the grid by more than one row.
const WHEEL_STEPS: usize = 40;

fn metrics() -> GridMetrics {
    GridMetrics {
        columns: 6,
        cell: vec2(240.0, 180.0),
        spacing: 8.0,
    }
}

/// A tall grid over [`SCROLL_ITEMS`] items.
fn scrolling_grid() -> VirtualGrid {
    VirtualGrid {
        metrics: metrics(),
        item_count: SCROLL_ITEMS,
    }
}

/// A headless screen with the given events.
fn input(events: Vec<Event>) -> RawInput {
    RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(900.0, 700.0))),
        events,
        ..Default::default()
    }
}

/// One small wheel step over the grid centre.
///
/// A point delta under 8 skips the egui wheel smoothing, so the offset moves now.
fn wheel_step() -> Vec<Event> {
    vec![
        Event::PointerMoved(pos2(450.0, 350.0)),
        Event::MouseWheel {
            unit: MouseWheelUnit::Point,
            delta: vec2(0.0, -WHEEL_STEP),
            phase: TouchPhase::Move,
            modifiers: Modifiers::NONE,
        },
    ]
}

/// Draw one interactive frame and return the painted item indices.
fn draw(ctx: &Context, raw: RawInput) -> Vec<usize> {
    let mut painted = Vec::new();
    let grid = scrolling_grid();
    ctx.run_ui(raw, |ui| {
        grid.show_interactive(ui, |_, _, index| painted.push(index));
    })
    .drop_without_applying_deltas();
    painted
}

/// Draw one frame per wheel step and return the last painted set.
fn scroll(ctx: &Context, frames: usize) -> Vec<usize> {
    let mut painted = Vec::new();
    for _ in 0..frames {
        painted = draw(ctx, input(wheel_step()));
    }
    painted
}

#[test]
fn row_count_rounds_up() {
    assert_eq!(metrics().row_count(0), 0);
    assert_eq!(metrics().row_count(1), 1);
    assert_eq!(metrics().row_count(6), 1);
    assert_eq!(metrics().row_count(7), 2);
    assert_eq!(metrics().row_count(10_000), 1667);
}

#[test]
fn content_size_holds_all_rows() {
    let size = metrics().content_size(10_000);
    assert_eq!(size.x, 6.0 * 248.0 - 8.0);
    assert_eq!(size.y, 1667.0 * 188.0 - 8.0);
    assert_eq!(metrics().content_size(0), Vec2::ZERO);
}

#[test]
fn visible_rows_cover_the_viewport() {
    let metrics = metrics();
    let viewport = Rect::from_min_size(pos2(0.0, 0.0), vec2(1480.0, 900.0));
    assert_eq!(metrics.visible_rows(viewport), 0..5);

    let scrolled = Rect::from_min_size(pos2(0.0, 188.0), vec2(1480.0, 900.0));
    assert_eq!(metrics.visible_rows(scrolled), 1..6);
}

#[test]
fn visible_cols_stay_inside_the_column_count() {
    let metrics = metrics();
    let wide = Rect::from_min_size(pos2(0.0, 0.0), vec2(5000.0, 900.0));
    assert_eq!(metrics.visible_cols(wide), 0..6);
    let narrow = Rect::from_min_size(pos2(0.0, 0.0), vec2(500.0, 900.0));
    assert_eq!(metrics.visible_cols(narrow), 0..3);
}

#[test]
fn cell_rect_uses_the_stride() {
    let rect = metrics().cell_rect(2, 3);
    assert_eq!(rect.min, pos2(3.0 * 248.0, 2.0 * 188.0));
    assert_eq!(rect.size(), vec2(240.0, 180.0));
}

#[test]
fn zero_columns_show_nothing() {
    let metrics = GridMetrics {
        columns: 0,
        ..metrics()
    };
    assert_eq!(metrics.row_count(10), 0);
    assert_eq!(metrics.content_size(10), Vec2::ZERO);
}

#[test]
fn the_cell_size_comes_from_the_stylesheet() {
    assert_eq!(cell_size(), vec2(196.0, 208.0));
    assert_eq!(cell_size(), metrics_for(616.0).cell);
    assert_eq!(metrics_for(616.0).spacing, tokens::GRID_GAP);
}

#[test]
fn metrics_for_fills_the_available_width() {
    // Three 196 point tracks and two 14 point gaps need 616 points.
    assert_eq!(metrics_for(616.0).columns, 3);
    assert_eq!(metrics_for(615.0).columns, 2);
    assert_eq!(
        metrics_for(0.0).columns,
        1,
        "a narrow grid keeps one column"
    );
    assert_eq!(metrics_for(-50.0).columns, 1);
    assert_eq!(metrics_for(5000.0).columns, 23);
}

#[test]
fn the_cell_width_stretches_to_fill_the_row() {
    // Four tracks at the shell grid width: (874 - 3 * 14) / 4.
    let shell = metrics_for(874.0);
    assert_eq!(shell.columns, 4);
    assert!((shell.cell.x - 208.0).abs() < f32::EPSILON);
    assert!(
        (shell.cell.y - 208.0).abs() < f32::EPSILON,
        "the height stays"
    );

    // A 196 point track divides 616 points exactly.
    let exact = metrics_for(616.0);
    assert_eq!(exact.cell.x, 196.0);

    // The last column ends on the right edge of the row.
    let width = exact.cell.x * 3.0 + exact.spacing * 2.0;
    assert!((width - 616.0).abs() < f32::EPSILON);
}

#[test]
fn a_stretched_track_never_goes_below_the_stylesheet_width() {
    let narrow = metrics_for(100.0);
    assert_eq!(narrow.columns, 1);
    assert_eq!(narrow.cell.x, 196.0, "the track keeps its minimum width");
}

#[test]
fn a_wheel_turn_scrolls_the_grid() {
    let ctx = Context::default();
    let first = draw(&ctx, input(Vec::new()));
    assert_eq!(first.first(), Some(&0), "the grid starts at the top");

    let scrolled = scroll(&ctx, WHEEL_STEPS);
    assert_ne!(scrolled, first, "the wheel painted the same cells");
    assert!(
        scrolled.first() > first.first(),
        "the first painted cell did not move down: {scrolled:?}"
    );
}

#[test]
fn a_quiet_frame_keeps_the_scroll_offset() {
    let ctx = Context::default();
    draw(&ctx, input(Vec::new()));
    let scrolled = scroll(&ctx, WHEEL_STEPS);
    assert_ne!(scrolled.first(), Some(&0), "the wheel did not scroll");

    let quiet = draw(&ctx, input(Vec::new()));
    assert_eq!(quiet, scrolled, "egui must keep the offset across frames");
}

/// Draw one frame at a pinned scroll offset and return the painted indices.
fn draw_pinned(ctx: &Context) -> Vec<usize> {
    let mut painted = Vec::new();
    let grid = scrolling_grid();
    ctx.run_ui(input(wheel_step()), |ui| {
        grid.show_at(ui, 0.0, |_, _, index| painted.push(index));
    })
    .drop_without_applying_deltas();
    painted
}

#[test]
fn a_pinned_offset_ignores_the_wheel() {
    let ctx = Context::default();
    let first = draw_pinned(&ctx);
    assert_eq!(first.first(), Some(&0), "the pinned grid starts at the top");

    for _ in 0..WHEEL_STEPS {
        let _ = draw_pinned(&ctx);
    }
    let last = draw_pinned(&ctx);
    assert_eq!(last, first, "a pinned offset cannot move by input");
}

/// Items tall enough to click a tile after a scroll.
fn offset_items() -> Vec<GridItem> {
    (0..SCROLL_ITEMS)
        .map(|index| {
            GridItem::new(
                PathBuf::from("/images").join(format!("{index}.png")),
                1,
                None,
            )
        })
        .collect()
}

#[test]
fn a_click_at_a_non_zero_offset_reports_the_tile_under_the_pointer() {
    let ctx = Context::default();
    let items = offset_items();
    let order: Vec<usize> = (0..items.len()).collect();
    let selection = Selection::default();
    let grid = VirtualGrid {
        metrics: metrics_for(900.0),
        item_count: items.len(),
    };
    let scroll_y = 500.0;

    let mut painted = 0;
    ctx.run_ui(input(Vec::new()), |ui| {
        painted = grid
            .show_items_at(ui, scroll_y, &items, &order, &selection)
            .painted;
    })
    .drop_without_applying_deltas();
    assert!(painted > 0, "the pinned grid painted nothing");

    let (index, rect) = (0..items.len())
        .find_map(|index| {
            ctx.read_response(Id::new(("tarragon-grid-tile", index)))
                .map(|response| (index, response.rect))
        })
        .expect("a tile registered");
    assert_ne!(index, 0, "the offset must move the first painted tile");

    let pos = rect.center();
    let press = |pressed| Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    };
    let mut click = None;
    for events in [
        vec![Event::PointerMoved(pos)],
        vec![press(true)],
        vec![press(false)],
    ] {
        let mut outcome = GridResponse::default();
        ctx.run_ui(input(events), |ui| {
            outcome = grid.show_items_at(ui, scroll_y, &items, &order, &selection);
        })
        .drop_without_applying_deltas();
        click = outcome.click.or(click);
    }
    assert_eq!(click.map(|click| click.index), Some(index));
}
