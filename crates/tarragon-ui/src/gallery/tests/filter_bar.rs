//! Tests for the filter bar in the gallery strip.

use super::*;
use crate::test_support;
use egui::epaint::Shape;
use tarragon_core::color::ColorBucket;
use tarragon_core::filter_bar::ColorChoice;

#[test]
fn a_colour_click_flags_one_change() {
    let ctx = test_support::context();
    crate::theme::apply_theme(&ctx);
    let mut gallery = Gallery::new(SortMode::Natural);
    let first = frame(&ctx, &mut gallery, Vec::new());
    let swatch = first
        .shapes
        .iter()
        .find_map(|c| match &c.shape {
            Shape::Rect(r) if r.rect.size() == vec2(36.0, 36.0) => Some(r.rect.center()),
            _ => None,
        })
        .expect("a swatch was painted");
    first.drop_without_applying_deltas();
    assert_eq!(gallery.take_filter_change(), None);
    let button = |pressed| Event::PointerButton {
        pos: swatch,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    };
    let events = vec![Event::PointerMoved(swatch), button(true), button(false)];
    frame(&ctx, &mut gallery, events).drop_without_applying_deltas();
    let (state, scope) = gallery.take_filter_change().expect("a change");
    assert_eq!(state.colors.len(), 1);
    assert_eq!(scope, FilterScope::Folder);
    assert_eq!(gallery.take_filter_change(), None);
}

#[test]
fn a_scope_switch_flags_a_change() {
    let mut gallery = Gallery::new(SortMode::Natural);
    gallery.set_scope(Scope::All);
    let change = gallery.take_filter_change().map(|c| c.1);
    assert_eq!(change, Some(FilterScope::Global));
    assert_eq!(gallery.take_filter_change(), None);
}

#[test]
fn disabling_colours_clears_them_and_flags_a_change() {
    let mut gallery = Gallery::new(SortMode::Natural);
    gallery.set_colors_enabled(false);
    assert_eq!(gallery.take_filter_change(), None);
    gallery.set_colors_enabled(true);
    gallery
        .bar
        .toggle_color(ColorChoice::Bucket(ColorBucket::Red));
    gallery.set_colors_enabled(false);
    let (state, _) = gallery.take_filter_change().expect("a change");
    assert!(state.colors.is_empty() && !state.transparent);
}
