//! Tests for the busy strip in the gallery frame.

use std::time::Duration;

use super::*;
use egui::epaint::Shape;

/// The texts in `shape`, in paint order.
fn texts_in(shape: &Shape, out: &mut Vec<String>) {
    match shape {
        Shape::Vec(shapes) => shapes.iter().for_each(|s| texts_in(s, out)),
        Shape::Text(t) => out.push(t.galley.job.text.clone()),
        _ => {}
    }
}

/// How many times the text `Working...` shows in one frame, and the repaint
/// delay that the frame asked for.
fn busy_frame(ctx: &egui::Context, gallery: &mut Gallery) -> (usize, Duration) {
    let output = frame(ctx, gallery, Vec::new());
    let mut texts = Vec::new();
    for clipped in &output.shapes {
        texts_in(&clipped.shape, &mut texts);
    }
    let delay = output
        .viewport_output
        .values()
        .map(|viewport| viewport.repaint_delay)
        .min()
        .expect("the frame reports a repaint delay");
    output.drop_without_applying_deltas();
    (texts.iter().filter(|t| *t == "Working...").count(), delay)
}

/// A themed context and a gallery that shows the folder `/images`.
fn setup() -> (egui::Context, Gallery) {
    let ctx = egui::Context::default();
    crate::theme::apply_theme(&ctx);
    let mut gallery = Gallery::new(SortMode::Natural);
    gallery.set_folder(Some(PathBuf::from("/images")));
    gallery.set_items(items());
    (ctx, gallery)
}

#[test]
fn the_strip_text_shows_once_while_busy_and_not_when_idle() {
    let (ctx, mut gallery) = setup();
    assert_eq!(busy_frame(&ctx, &mut gallery).0, 0);
    gallery.set_busy(true);
    assert_eq!(busy_frame(&ctx, &mut gallery).0, 1);
    gallery.set_busy(false);
    assert_eq!(busy_frame(&ctx, &mut gallery).0, 0);
}

#[test]
fn toggling_busy_keeps_the_grid_the_search_and_the_selection() {
    let (ctx, mut gallery) = setup();
    search(&mut gallery, "a");
    gallery.select_all_visible();
    let visible = gallery.visible_len();
    let selected = gallery.selected_paths();
    assert!(visible > 0 && !selected.is_empty());
    for busy in [true, false] {
        gallery.set_busy(busy);
        busy_frame(&ctx, &mut gallery);
        assert_eq!(gallery.visible_len(), visible);
        assert_eq!(gallery.selected_paths(), selected);
        assert_eq!(gallery.search, "a");
    }
}

#[test]
fn the_frame_asks_for_a_repaint_only_while_busy() {
    let (ctx, mut gallery) = setup();
    // The first frames settle fonts and layout, which ask for repaints too.
    for _ in 0..3 {
        busy_frame(&ctx, &mut gallery);
    }
    let (_, idle) = busy_frame(&ctx, &mut gallery);
    assert!(idle > Duration::from_secs(1), "idle asked for {idle:?}");
    gallery.set_busy(true);
    let (_, busy) = busy_frame(&ctx, &mut gallery);
    assert_eq!(busy, Duration::ZERO, "the strip animates");
}

/// The rects of the filter buttons and the top of the grid in one frame. The
/// grid is the lowest clip area, because it is the last part of the page.
fn layout(ctx: &egui::Context, gallery: &mut Gallery) -> (Vec<(String, egui::Pos2)>, f32) {
    let output = frame(ctx, gallery, Vec::new());
    let buttons = super::filter_bar::texts(&output)
        .into_iter()
        .filter(|t| t.0 == "Filter Tags" || t.0 == "Filter Folders")
        .collect();
    let grid_top = output
        .shapes
        .iter()
        .map(|clipped| clipped.clip_rect.min.y)
        .fold(f32::MIN, f32::max);
    output.drop_without_applying_deltas();
    (buttons, grid_top)
}

#[test]
fn the_strip_moves_the_grid_and_leaves_the_filter_buttons_still() {
    let (ctx, mut gallery) = setup();
    for _ in 0..3 {
        busy_frame(&ctx, &mut gallery);
    }
    let (idle_buttons, idle_top) = layout(&ctx, &mut gallery);
    assert!(!idle_buttons.is_empty(), "a filter button shows");
    gallery.set_busy(true);
    let (busy_buttons, busy_top) = layout(&ctx, &mut gallery);
    assert_eq!(busy_buttons, idle_buttons, "the filter buttons stay put");
    assert!(
        busy_top > idle_top,
        "the grid moves down: {idle_top} to {busy_top}"
    );
}
