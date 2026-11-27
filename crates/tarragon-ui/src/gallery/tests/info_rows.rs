//! Tests for the header count and the info row sharing the strip width.

use super::*;
use crate::test_support;
use egui::epaint::Shape;
use egui::{Frame, Margin, Rect};
use tarragon_core::color::ColorBucket;
use tarragon_core::filter_bar::ColorChoice;

const LONG_FOLDER: &str = "reference-sketches-and-studies";
const COUNT: &str = "6 files";
const SELECTED: &str = "1 selected";

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

#[test]
fn the_header_count_stays_whole_at_the_minimum_width() {
    for filters in [0, 2] {
        let painted = draw(260.0, filters);
        let count = rect_where(&painted, |t| t == COUNT);
        assert!(count.max.x <= 260.0, "{filters}: ends at {}", count.max.x);
        // The long folder name gives way before the count does.
        assert!(
            !painted.iter().any(|(t, _)| t == LONG_FOLDER),
            "{filters}: the whole folder name fits in 260 points"
        );
    }
}

#[test]
fn the_info_row_holds_the_controls_on_one_line_and_leaves_the_selection_out() {
    // The selection count lives in the status bar.
    for width in [260.0, 1200.0] {
        for filters in [0, 2] {
            let painted = draw(width, filters);
            assert!(
                !painted.iter().any(|(t, _)| t == SELECTED),
                "{width}, {filters}: the row paints the selection"
            );
            let sort = rect_where(&painted, |t| t == "Sort");
            let date = rect_where(&painted, |t| t == "Date");
            assert!(sort.min.y < date.max.y && date.min.y < sort.max.y);
        }
    }
}

#[test]
fn the_filter_pill_keeps_its_place_in_the_row() {
    for width in [260.0, 1200.0] {
        let painted = draw(width, 2);
        let filter = rect_where(&painted, |t| t == "2 filters active");
        let date = rect_where(&painted, |t| t == "Date");
        let sort = rect_where(&painted, |t| t == "Sort");
        assert!(
            date.max.x <= filter.min.x,
            "{width}: the pill is not after Date"
        );
        assert!(
            sort.max.x <= date.min.x,
            "{width}: Sort is not before the pills"
        );
    }
}

#[test]
fn the_header_keeps_its_gaps() {
    let wide = draw(600.0, 0);
    let crumb = rect_where(&wide, |t| t == "library");
    let heading = rect_where(&wide, |t| t == LONG_FOLDER);
    // The glyph boxes carry the line leading, so the tight boxes sit wider
    // than the 2 point gap; the theme's default 8 point gap would add six more.
    let gap = heading.min.y - crumb.max.y;
    assert!(gap < 18.0, "the breadcrumb sits {gap} above the heading");
}

/// A gallery of `count` items in the long folder, with a count that grows wide.
fn many(count: usize) -> Gallery {
    let mut gallery = Gallery::new(SortMode::Natural);
    let root = PathBuf::from("/library").join(LONG_FOLDER);
    let items = (0..count)
        .map(|n| {
            GridItem::new(
                root.join(format!("{n}.png")),
                i64::try_from(n).expect("a small index"),
                None,
            )
        })
        .collect();
    gallery.set_items(items);
    gallery.set_folder(Some(root));
    gallery
}

#[test]
fn a_wide_count_wraps_the_tabs_instead_of_overrunning() {
    for width in [180.0, 260.0] {
        let ctx = test_support::context();
        crate::theme::apply_theme(&ctx);
        let mut gallery = many(1_200);
        let output = ctx.run_ui(test_support::input(width, vec![]), |ui| {
            Frame::new()
                .inner_margin(Margin::same(8))
                .show(ui, |ui| gallery.show(ui));
        });
        let count = output
            .shapes
            .iter()
            .find_map(|clipped| match &clipped.shape {
                Shape::Text(shape) if shape.galley.job.text.ends_with("files") => {
                    Some(shape.visual_bounding_rect())
                }
                _ => None,
            })
            .expect("the count is painted");
        let track = output
            .shapes
            .iter()
            .filter_map(|clipped| match &clipped.shape {
                Shape::Rect(rect)
                    if rect.fill == tokens::BG_TERTIARY && rect.rect.min.y < 100.0 =>
                {
                    Some(rect.rect)
                }
                _ => None,
            })
            .max_by(|a, b| a.width().total_cmp(&b.width()))
            .expect("the segmented track is painted");
        output.drop_without_applying_deltas();
        assert!(
            !count.intersects(track),
            "{width}: the count {count:?} overlaps the track {track:?}"
        );
        // A pane too narrow for both puts the tabs on their own line.
        if width < 200.0 {
            assert!(
                track.min.y >= count.max.y,
                "{width}: the track starts at {} and the count ends at {}",
                track.min.y,
                count.max.y
            );
        }
    }
}

#[test]
fn the_scope_tabs_stay_inside_the_strip_padding() {
    for width in [260.0, 900.0] {
        let ctx = test_support::context();
        crate::theme::apply_theme(&ctx);
        let mut gallery = gallery(0);
        let output = ctx.run_ui(test_support::input(width, vec![]), |ui| {
            Frame::new()
                .inner_margin(Margin::same(8))
                .show(ui, |ui| gallery.show(ui));
        });
        // The segmented track is the widest field-filled rect of the header.
        let track = output
            .shapes
            .iter()
            .filter_map(|clipped| match &clipped.shape {
                Shape::Rect(rect)
                    if rect.fill == tokens::BG_TERTIARY && rect.rect.min.y < 100.0 =>
                {
                    Some(rect.rect)
                }
                _ => None,
            })
            .max_by(|a, b| a.width().total_cmp(&b.width()));
        output.drop_without_applying_deltas();
        let track = track.expect("the segmented track is painted");
        // The fixture insets the pane by 8 points and the strip by 8 more.
        assert!(
            track.max.x <= width - 16.0 + 0.5,
            "{width}: the track ends at {}",
            track.max.x
        );
    }
}
