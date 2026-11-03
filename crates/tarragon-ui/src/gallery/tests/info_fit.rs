//! Tests for the info line at a narrow gallery width.

use super::*;
use crate::test_support;
use egui::Rect;
use egui::epaint::Shape;

const LONG_FOLDER: &str = "reference-sketches-and-studies";
const COUNTS: &str = "6 files - 1 selected";

/// Six items in `folder`, one of them selected.
fn gallery_in(folder: &str, scope: Scope) -> Gallery {
    let mut gallery = Gallery::new(SortMode::Natural);
    let root = PathBuf::from("/library").join(folder);
    let items = (0..6)
        .map(|n| GridItem::new(root.join(format!("{n}.png")), n, None))
        .collect();
    gallery.set_items(items);
    gallery.set_folder(Some(root));
    gallery.set_scope(scope);
    gallery.selection.select_only(0);
    gallery
}

/// Draw one frame `width` points wide. Returns the painted info line and
/// its rect.
fn info_line(gallery: &mut Gallery, width: f32) -> (String, Rect) {
    let ctx = test_support::context();
    crate::theme::apply_theme(&ctx);
    let output = ctx.run_ui(test_support::input(width, vec![]), |ui| gallery.show(ui));
    let found = output
        .shapes
        .iter()
        .find_map(|clipped| match &clipped.shape {
            Shape::Text(shape) if shape.galley.job.text.contains("6 files") => {
                let shown: String = shape.galley.rows.iter().map(|r| r.text()).collect();
                Some((shown, shape.visual_bounding_rect()))
            }
            _ => None,
        });
    output.drop_without_applying_deltas();
    found.expect("the info line was painted")
}

#[test]
fn a_long_folder_name_gives_way_to_the_counts_at_the_minimum_width() {
    let mut gallery = gallery_in(LONG_FOLDER, Scope::Folder);
    let (line, rect) = info_line(&mut gallery, 260.0);
    assert!(line.ends_with(COUNTS), "{line:?}");
    assert!(line.contains('…'), "{line:?}");
    assert!(!line.contains(LONG_FOLDER), "{line:?}");
    assert!(rect.max.x <= 260.0, "{line:?} ends at {}", rect.max.x);
}

#[test]
fn the_full_line_shows_at_a_wide_width() {
    let mut gallery = gallery_in(LONG_FOLDER, Scope::Folder);
    let (line, _) = info_line(&mut gallery, 1200.0);
    assert_eq!(line, format!("{LONG_FOLDER} - {COUNTS}"));
}

#[test]
fn the_all_images_line_follows_the_same_rule() {
    let mut gallery = gallery_in(LONG_FOLDER, Scope::All);
    let (wide, _) = info_line(&mut gallery, 1200.0);
    assert_eq!(wide, format!("All Images - {COUNTS}"));

    let (line, rect) = info_line(&mut gallery, 260.0);
    assert!(line.ends_with(COUNTS), "{line:?}");
    assert!(line.contains('…'), "{line:?}");
    assert!(rect.max.x <= 260.0, "{line:?} ends at {}", rect.max.x);
}

#[test]
fn the_counts_truncate_last_when_they_do_not_fit() {
    let measure = |text: &str| f32::from(u16::try_from(text.chars().count()).unwrap_or(u16::MAX));
    let fitted = super::super::info::fit_name("folder", " - 6 files - 1 selected", 10.0, measure);
    assert!(fitted.starts_with('…'), "{fitted:?}");
    assert!(!fitted.contains("folder"), "{fitted:?}");
}
