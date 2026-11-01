//! Tests for the filter pill in the info bar.

use super::*;
use crate::test_support;
use egui::Rect;
use egui::epaint::Shape;
use tarragon_core::color::ColorBucket;
use tarragon_core::filter_state::{FilterScope, FilterState};

/// The filter count that the info bar shows for `state` in `scope`.
fn count(state: &FilterState, scope: Scope) -> usize {
    let scope = match scope {
        Scope::All => FilterScope::Global,
        Scope::Folder => FilterScope::Folder,
    };
    state.active_count(scope)
}

/// Draw one frame `width` points wide with `filters` active. Returns the
/// text that the frame painted, each with its rect.
fn draw(width: f32, filters: usize) -> Vec<(String, Rect)> {
    let ctx = test_support::context();
    crate::theme::apply_theme(&ctx);
    let mut gallery = Gallery::new(SortMode::Natural);
    gallery.set_filter_count(filters);
    let output = ctx.run_ui(test_support::input(width, vec![]), |ui| gallery.show(ui));
    let text = output
        .shapes
        .iter()
        .filter_map(|clipped| match &clipped.shape {
            Shape::Text(shape) => {
                Some((shape.galley.job.text.clone(), shape.visual_bounding_rect()))
            }
            _ => None,
        })
        .collect();
    output.drop_without_applying_deltas();
    text
}

#[test]
fn no_pill_shows_at_zero_filters() {
    let text = draw(600.0, 0);
    assert!(!text.iter().any(|(t, _)| t.ends_with("active")));
}

#[test]
fn the_pill_counts_two_filters_and_one_filter() {
    for (filters, expected) in [(2, "2 filters active"), (1, "1 filter active")] {
        let text = draw(600.0, filters);
        assert!(text.iter().any(|(t, _)| t == expected), "{expected}");
    }
}

#[test]
fn pill_text_for_search_counts_once() {
    let mut gallery = Gallery::new(SortMode::Natural);
    for typed in ["cat", "   "] {
        search(&mut gallery, typed);
        let state = FilterState {
            text: gallery.applied_search.clone(),
            ..FilterState::default()
        };
        assert_eq!(count(&state, gallery.scope()), 1, "{typed:?}");
    }
    assert_eq!(filter_pill_text(1).as_deref(), Some("1 filter active"));
}

#[test]
fn pill_text_counts_folder_chips_in_all_images_only() {
    let mut state = FilterState::default();
    state.folders.insert(sketches());
    state.colors.insert(ColorBucket::Red);
    assert_eq!(count(&state, Scope::Folder), 1);
    assert_eq!(count(&state, Scope::All), 2);
    assert_eq!(filter_pill_text(2).as_deref(), Some("2 filters active"));
}

/// The rect of the painted text `text`.
fn rect_of(painted: &[(String, Rect)], text: &str) -> Rect {
    let found = painted.iter().find(|(t, _)| t == text);
    let (_, rect) = found.unwrap_or_else(|| panic!("{text} was not painted"));
    *rect
}

#[test]
fn the_pill_and_the_sort_pills_keep_their_slots_at_a_narrow_width() {
    for width in [260.0, 245.0] {
        let painted = draw(width, 2);
        let filter = rect_of(&painted, "2 filters active");
        let date = rect_of(&painted, "Date");
        let name = rect_of(&painted, "Name");
        let sort = rect_of(&painted, "Sort");
        let info = painted
            .iter()
            .find(|(t, _)| t.ends_with("files"))
            .map(|(_, rect)| *rect)
            .expect("the info text was painted");
        assert!(name.max.x <= date.min.x, "{width}: the sort pills overlap");
        assert!(
            date.max.x <= filter.min.x,
            "{width}: the pill overlaps Date"
        );
        assert!(sort.max.x <= name.min.x, "{width}: Sort overlaps Name");
        assert!(info.max.x <= sort.min.x, "{width}: the info text overlaps");
    }
}
