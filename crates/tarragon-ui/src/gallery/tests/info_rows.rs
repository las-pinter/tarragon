//! Tests for the info line and the sort controls sharing the strip rows.

use super::*;
use crate::test_support;
use egui::epaint::Shape;
use egui::{Frame, Margin, Rect};
use tarragon_core::color::ColorBucket;
use tarragon_core::filter_bar::ColorChoice;

const LONG_FOLDER: &str = "reference-sketches-and-studies";
const WHOLE_COUNTS: &str = "6 files - 1 selected";

/// Six items in a long folder, one selected, and `filters` colour filters.
fn gallery(filters: usize) -> Gallery {
    let mut gallery = Gallery::new(SortMode::Natural);
    let root = PathBuf::from("/library").join(LONG_FOLDER);
    let items = (0..6)
        .map(|n| GridItem::new(root.join(format!("{n}.png")), n, None))
        .collect();
    gallery.set_items(items);
    gallery.set_folder(Some(root));
    gallery.selection.select_only(0);
    for bucket in [ColorBucket::Red, ColorBucket::Blue]
        .into_iter()
        .take(filters)
    {
        gallery.bar.toggle_color(ColorChoice::Bucket(bucket));
    }
    gallery
}

/// The painted text of one frame `width` points wide, each with its rect.
fn draw(width: f32, filters: usize) -> Vec<(String, Rect)> {
    let ctx = test_support::context();
    crate::theme::apply_theme(&ctx);
    let mut gallery = gallery(filters);
    // The dock insets the Gallery pane by 8 points, with no constant to reuse.
    let output = ctx.run_ui(test_support::input(width, vec![]), |ui| {
        Frame::new()
            .inner_margin(Margin::same(8))
            .show(ui, |ui| gallery.show(ui));
    });
    let text = output
        .shapes
        .iter()
        .filter_map(|clipped| match &clipped.shape {
            Shape::Text(shape) => {
                let shown: String = shape.galley.rows.iter().map(|r| r.text()).collect();
                Some((shown, shape.visual_bounding_rect()))
            }
            _ => None,
        })
        .collect();
    output.drop_without_applying_deltas();
    text
}

/// The rect of the painted text for which `pick` is true.
fn rect_where(painted: &[(String, Rect)], pick: impl Fn(&str) -> bool) -> Rect {
    painted
        .iter()
        .find(|(text, _)| pick(text))
        .map(|(_, rect)| *rect)
        .expect("the text was painted")
}

/// The info line text and rect, the Sort label rect, and the Name pill rect.
fn rows(width: f32, filters: usize) -> (String, Rect, Rect, Rect) {
    let painted = draw(width, filters);
    let is_info = |t: &str| t.contains("6 files");
    let info = rect_where(&painted, is_info);
    let text = painted.iter().find(|(t, _)| is_info(t)).map(|(t, _)| t);
    let sort = rect_where(&painted, |t| t == "Sort");
    let name = rect_where(&painted, |t| t == "Name");
    (text.cloned().unwrap_or_default(), info, sort, name)
}

#[test]
fn the_counts_stay_whole_at_the_minimum_width() {
    for filters in [0, 2] {
        let (line, info, _, _) = rows(260.0, filters);
        assert!(line.ends_with(WHOLE_COUNTS), "{filters}: {line:?}");
        assert!(info.max.x <= 260.0, "{filters}: ends at {}", info.max.x);
    }
}

#[test]
fn the_controls_drop_below_the_info_when_they_do_not_fit_beside_it() {
    for filters in [0, 2] {
        let (_, info, sort, name) = rows(260.0, filters);
        assert!(info.max.y <= sort.min.y, "{filters}: Sort is not lower");
        assert!(info.max.y <= name.min.y, "{filters}: Name is not lower");
    }
}

#[test]
fn the_filter_pill_keeps_its_place_in_the_lower_row() {
    let painted = draw(260.0, 2);
    let filter = rect_where(&painted, |t| t == "2 filters active");
    let date = rect_where(&painted, |t| t == "Date");
    let sort = rect_where(&painted, |t| t == "Sort");
    let info = rect_where(&painted, |t| t.contains("6 files"));
    assert!(date.max.x <= filter.min.x, "the pill is not after Date");
    assert!(sort.max.x <= date.min.x, "Sort is not before the pills");
    assert!(info.max.y <= filter.min.y, "the pill is not lower");
}

#[test]
fn everything_shares_one_row_at_a_wide_width() {
    for filters in [0, 2] {
        let (line, info, sort, name) = rows(1200.0, filters);
        assert_eq!(line, format!("{LONG_FOLDER} - {WHOLE_COUNTS}"));
        assert!(info.min.y < sort.max.y && sort.min.y < info.max.y);
        assert!(info.min.y < name.max.y && name.min.y < info.max.y);
    }
}
