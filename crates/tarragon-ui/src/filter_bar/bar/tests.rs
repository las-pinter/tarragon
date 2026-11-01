//! Tests for the filter bar rows.

use super::show;
use crate::swatch::FILTER_SIZE;
use crate::test_support::input;
use egui::{Context, epaint::Shape};
use tarragon_core::filter_bar::FilterBar;
use tarragon_core::filter_state::FilterScope;

/// Draw the bar. Returns the used height and the painted texts with their y.
fn draw(scope: FilterScope, colors: bool, bar: &mut FilterBar) -> (f32, Vec<(String, f32)>) {
    let ctx = Context::default();
    let mut height = 0.0;
    let out = ctx.run_ui(input(600.0, vec![]), |ui| {
        show(ui, bar, &[], &[], scope, colors);
        height = ui.min_rect().height();
    });
    let mut texts = Vec::new();
    for clipped in &out.shapes {
        if let Shape::Text(t) = &clipped.shape {
            texts.push((t.galley.text().to_owned(), t.pos.y));
        }
    }
    out.drop_without_applying_deltas();
    (height, texts)
}

#[test]
fn rows_draw_in_order_and_empty_rows_take_no_height() {
    let mut bar = FilterBar::default();
    let (full, texts) = draw(FilterScope::Global, true, &mut bar);
    let y = |name: &str| texts.iter().find(|t| t.0 == name).expect(name).1;
    assert!(y("Filter Tags") < y("Filter Folders"));
    let (no_colors, plain) = draw(FilterScope::Global, false, &mut bar);
    let top = |name: &str| plain.iter().find(|t| t.0 == name).expect(name).1;
    assert!(
        y("Filter Tags") - top("Filter Tags") >= FILTER_SIZE,
        "colours draw first"
    );
    let (no_folders, texts) = draw(FilterScope::Folder, false, &mut bar);
    assert!(!texts.iter().any(|t| t.0 == "Filter Folders"));
    assert!(full > no_colors && no_colors > no_folders);
    let (empty, _) = draw(FilterScope::Folder, false, &mut bar);
    assert!((no_folders - empty).abs() < f32::EPSILON);
}

#[test]
fn rows_are_twelve_points_apart() {
    let mut bar = FilterBar::default();
    let (global, _) = draw(FilterScope::Global, false, &mut bar);
    let (folder, _) = draw(FilterScope::Folder, false, &mut bar);
    // The info bar space less the parent spacing, then 4 pt padding twice.
    let pads = 4.0 - egui::Spacing::default().item_spacing.y + 8.0;
    let row = folder - pads;
    assert!(
        (global - folder - row - 12.0).abs() < 0.01,
        "{global} {folder}"
    );
}

#[test]
fn the_first_row_starts_8_pt_below_and_4_pt_inside() {
    let ctx = Context::default();
    let mut bar = FilterBar::default();
    let (mut above, mut left) = (0.0, 0.0);
    let out = ctx.run_ui(input(600.0, vec![]), |ui| {
        let label = ui.label("info");
        (above, left) = (label.rect.max.y, label.rect.min.x);
        show(ui, &mut bar, &[], &[], FilterScope::Folder, true);
    });
    let swatch = out.shapes.iter().find_map(|c| match &c.shape {
        Shape::Rect(r) if r.rect.size() == egui::vec2(FILTER_SIZE, FILTER_SIZE) => Some(r.rect),
        _ => None,
    });
    out.drop_without_applying_deltas();
    let swatch = swatch.expect("a swatch was painted");
    assert!(
        (swatch.min.y - above - 8.0).abs() < 0.01,
        "{swatch:?} {above}"
    );
    assert!(
        (swatch.min.x - left - 4.0).abs() < 0.01,
        "{swatch:?} {left}"
    );
}
