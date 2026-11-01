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
    let events = test_support::click(swatch);
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

/// Every text drawn in a frame.
fn texts(out: &egui::FullOutput) -> Vec<(String, Pos2)> {
    let text = |s: &egui::epaint::ClippedShape| match &s.shape {
        Shape::Text(t) => Some((t.galley.text().to_owned(), t.pos)),
        _ => None,
    };
    out.shapes.iter().filter_map(text).collect()
}

/// Click the button with this label and return the texts of the next frame.
fn click_button(ctx: &egui::Context, gallery: &mut Gallery, label: &str) -> Vec<String> {
    let first = frame(ctx, gallery, Vec::new());
    let at = texts(&first)
        .into_iter()
        .find(|t| t.0 == label)
        .expect(label)
        .1
        + vec2(2.0, 2.0);
    first.drop_without_applying_deltas();
    let events = test_support::click(at);
    frame(ctx, gallery, events).drop_without_applying_deltas();
    let out = frame(ctx, gallery, Vec::new());
    let found = texts(&out).into_iter().map(|t| t.0).collect();
    out.drop_without_applying_deltas();
    found
}

/// A global-scope gallery with both lists set.
fn listed() -> (egui::Context, Gallery) {
    let ctx = test_support::context();
    crate::theme::apply_theme(&ctx);
    let mut gallery = Gallery::new(SortMode::Natural);
    gallery.set_scope(Scope::All);
    gallery.take_filter_change();
    gallery.set_filter_lists(vec!["lighting".into()], vec!["/home/u/sketches".into()]);
    (ctx, gallery)
}

#[test]
fn the_filter_lists_fill_the_menu_rows() {
    for (label, row) in [
        ("Filter Tags", "lighting"),
        ("Filter Folders", "u/sketches"),
    ] {
        let (ctx, mut gallery) = listed();
        assert!(click_button(&ctx, &mut gallery, label).contains(&row.into()));
    }
}

#[test]
fn opening_a_menu_reports_once() {
    for label in ["Filter Tags", "Filter Folders"] {
        let (ctx, mut gallery) = listed();
        assert!(!gallery.take_filter_menu_opened());
        click_button(&ctx, &mut gallery, label);
        assert!(gallery.take_filter_menu_opened(), "{label}");
        assert!(!gallery.take_filter_menu_opened(), "{label}");
    }
}

#[test]
fn an_open_menu_and_a_close_flag_no_change() {
    let (ctx, mut gallery) = listed();
    let open = click_button(&ctx, &mut gallery, "Filter Tags");
    assert!(open.contains(&"lighting".into()));
    assert_eq!(gallery.take_filter_change(), None);
    gallery.take_filter_menu_opened();
    let closed = click_button(&ctx, &mut gallery, "Filter Tags");
    assert!(!closed.contains(&"lighting".into()));
    assert_eq!(gallery.take_filter_change(), None);
    assert!(!gallery.take_filter_menu_opened());
}

#[test]
fn a_frame_with_an_open_menu_reports_no_new_open() {
    let (ctx, mut gallery) = listed();
    click_button(&ctx, &mut gallery, "Filter Tags");
    gallery.take_filter_menu_opened();
    frame(&ctx, &mut gallery, Vec::new()).drop_without_applying_deltas();
    assert!(!gallery.take_filter_menu_opened());
}

#[test]
fn escape_closes_an_open_menu() {
    for (label, row) in [
        ("Filter Tags", "lighting"),
        ("Filter Folders", "u/sketches"),
    ] {
        let (ctx, mut gallery) = listed();
        let open = click_button(&ctx, &mut gallery, label);
        assert!(open.contains(&row.into()), "{label}");
        let esc = Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        };
        frame(&ctx, &mut gallery, vec![esc]).drop_without_applying_deltas();
        let out = frame(&ctx, &mut gallery, Vec::new());
        let found: Vec<_> = texts(&out).into_iter().map(|t| t.0).collect();
        out.drop_without_applying_deltas();
        assert!(!found.contains(&row.into()), "{label}");
    }
}
