//! Tests for the empty card rule.

use super::*;
use std::collections::HashSet;
use tarragon_core::color::ColorBucket;
use tarragon_core::filter_bar::ColorChoice;

/// A gallery in the folder scope with four items in `/images`.
fn filled() -> Gallery {
    let mut gallery = Gallery::new(SortMode::Natural);
    gallery.set_folder(Some(PathBuf::from("/images")));
    gallery.set_items(items());
    gallery
}

#[test]
fn a_new_gallery_has_no_folder_and_loading_hides_the_card() {
    let mut gallery = Gallery::new(SortMode::Natural);
    assert_eq!(gallery.empty_kind(), Some(EmptyKind::NoFolder));
    gallery.set_loading(true);
    assert_eq!(gallery.empty_kind(), None);
    gallery.set_loading(false);
    assert_eq!(gallery.empty_kind(), Some(EmptyKind::NoFolder));
}

#[test]
fn a_folder_without_items_is_an_empty_folder() {
    let mut gallery = Gallery::new(SortMode::Natural);
    gallery.set_folder(Some(PathBuf::from("/images")));
    assert_eq!(gallery.empty_kind(), Some(EmptyKind::EmptyFolder));
}

#[test]
fn a_folder_with_items_outside_it_is_an_empty_folder() {
    let mut gallery = Gallery::new(SortMode::Natural);
    gallery.set_folder(Some(PathBuf::from("/other")));
    gallery.set_items(items());
    assert_eq!(gallery.empty_kind(), Some(EmptyKind::EmptyFolder));
}

#[test]
fn a_search_that_matches_nothing_is_no_result() {
    let mut gallery = filled();
    search(&mut gallery, "zzz");
    assert_eq!(gallery.empty_kind(), Some(EmptyKind::NoResult));
}

#[test]
fn an_empty_filter_result_is_no_result() {
    let mut gallery = filled();
    gallery.set_filter_result(Some(HashSet::new()));
    assert_eq!(gallery.empty_kind(), Some(EmptyKind::NoResult));
}

#[test]
fn a_colour_chip_alone_is_no_result() {
    let mut gallery = Gallery::new(SortMode::Natural);
    gallery.set_folder(Some(PathBuf::from("/images")));
    gallery
        .bar
        .toggle_color(ColorChoice::Bucket(ColorBucket::Red));
    assert_eq!(gallery.empty_kind(), Some(EmptyKind::NoResult));
}

#[test]
fn a_transparent_chip_alone_is_no_result() {
    let mut gallery = Gallery::new(SortMode::Natural);
    gallery.set_folder(Some(PathBuf::from("/images")));
    gallery.bar.state.transparent = true;
    assert_eq!(gallery.empty_kind(), Some(EmptyKind::NoResult));
}

#[test]
fn a_tag_chip_alone_is_no_result() {
    let mut gallery = Gallery::new(SortMode::Natural);
    gallery.set_folder(Some(PathBuf::from("/images")));
    gallery.bar.toggle_tag("ref");
    assert_eq!(gallery.empty_kind(), Some(EmptyKind::NoResult));
}

#[test]
fn a_folder_chip_in_scope_all_is_no_result() {
    let mut gallery = Gallery::new(SortMode::Natural);
    gallery.set_scope(Scope::All);
    gallery.bar.toggle_folder(Path::new("/images"));
    assert_eq!(gallery.empty_kind(), Some(EmptyKind::NoResult));
}

#[test]
fn a_stale_folder_chip_in_folder_scope_does_not_filter() {
    let mut gallery = Gallery::new(SortMode::Natural);
    gallery.set_folder(Some(PathBuf::from("/images")));
    gallery.bar.toggle_folder(Path::new("/images"));
    assert_eq!(gallery.empty_kind(), Some(EmptyKind::EmptyFolder));
}

#[test]
fn scope_all_without_items_is_no_folder_and_with_a_search_is_no_result() {
    let mut gallery = Gallery::new(SortMode::Natural);
    gallery.set_scope(Scope::All);
    assert_eq!(gallery.empty_kind(), Some(EmptyKind::NoFolder));
    search(&mut gallery, "a");
    assert_eq!(gallery.empty_kind(), Some(EmptyKind::NoResult));
}

#[test]
fn a_visible_item_gives_no_card_in_every_scope() {
    let mut gallery = filled();
    assert_eq!(gallery.empty_kind(), None);
    gallery.set_scope(Scope::All);
    assert_eq!(gallery.empty_kind(), None);
}

/// Run one frame with `events`. Return each painted text with its rect.
fn texts(
    ctx: &egui::Context,
    gallery: &mut Gallery,
    events: Vec<Event>,
) -> Vec<(String, egui::Rect)> {
    let output = frame(ctx, gallery, events);
    let found = output
        .shapes
        .iter()
        .filter_map(|clipped| match &clipped.shape {
            egui::epaint::Shape::Text(shape) => {
                Some((shape.galley.job.text.clone(), shape.visual_bounding_rect()))
            }
            _ => None,
        })
        .collect();
    output.drop_without_applying_deltas();
    found
}

/// A context with the app fonts and theme.
fn themed() -> egui::Context {
    let ctx = crate::test_support::context();
    crate::theme::apply_theme(&ctx);
    ctx
}

/// Whether some painted text equals `wanted`.
fn has(found: &[(String, egui::Rect)], wanted: &str) -> bool {
    found.iter().any(|(text, _)| text == wanted)
}

#[test]
fn a_new_gallery_paints_the_no_folder_card_with_its_button() {
    let mut gallery = Gallery::new(SortMode::Natural);
    let found = texts(&themed(), &mut gallery, Vec::new());
    assert!(has(&found, "Open a folder to begin"));
    assert!(
        found
            .iter()
            .any(|(text, _)| text.starts_with("Use File - Open Folder"))
    );
    assert!(has(&found, "Open Folder..."));
}

#[test]
fn an_empty_folder_lists_the_supported_extensions() {
    let mut gallery = Gallery::new(SortMode::Natural);
    gallery.set_folder(Some(PathBuf::from("/images")));
    let found = texts(&themed(), &mut gallery, Vec::new());
    assert!(has(&found, "No supported images in this folder"));
    assert!(has(&found, "Open Another Folder..."));
    let hint = found
        .iter()
        .find_map(|(text, _)| text.strip_prefix("This folder has no file"))
        .expect("the hint is painted");
    assert!(hint.ends_with("Supported: JPG, JPEG, PNG, WEBP, TIFF, TIF, PSD, PSB, CLIP, KRA."));
    assert!(!hint.contains("GIF") && !hint.contains("BMP"));
}

#[test]
fn a_no_result_gallery_paints_its_text_and_no_button() {
    let mut gallery = filled();
    search(&mut gallery, "zzz");
    let found = texts(&themed(), &mut gallery, Vec::new());
    assert!(has(&found, "No images match"));
    assert!(
        found
            .iter()
            .any(|(text, _)| text.starts_with("Change or clear the search"))
    );
    assert!(!found.iter().any(|(text, _)| text.ends_with("...")));
}

#[test]
fn a_gallery_with_items_paints_no_empty_text() {
    let mut gallery = filled();
    let found = texts(&themed(), &mut gallery, Vec::new());
    assert!(!has(&found, "Open a folder to begin"));
    assert!(!has(&found, "No images match"));
    assert!(!has(&found, "No supported images in this folder"));
}

#[test]
fn a_click_on_the_button_sets_the_request_once() {
    let ctx = themed();
    let mut gallery = Gallery::new(SortMode::Natural);
    let found = texts(&ctx, &mut gallery, Vec::new());
    let (_, rect) = found
        .iter()
        .find(|(text, _)| text == "Open Folder...")
        .expect("the button is painted");
    assert!(!gallery.take_open_folder_request());
    for events in crate::test_support::click(rect.center()) {
        frame(&ctx, &mut gallery, vec![events]).drop_without_applying_deltas();
    }
    frame(&ctx, &mut gallery, Vec::new()).drop_without_applying_deltas();
    assert!(gallery.take_open_folder_request());
    assert!(!gallery.take_open_folder_request());
}

/// The height of the click target that holds the text at `at`.
fn click_target_height(ctx: &egui::Context, at: egui::Pos2) -> f32 {
    let layer = ctx.layer_id_at(at).expect("a layer holds the text");
    ctx.viewport(|viewport| {
        viewport
            .prev_pass
            .widgets
            .get_layer(layer)
            .filter(|widget| widget.sense.senses_click() && widget.rect.contains(at))
            .map(|widget| widget.rect.height())
            .fold(f32::MAX, f32::min)
    })
}

#[test]
fn the_action_button_is_30_points_high() {
    let ctx = themed();
    let mut height = 0.0;
    let output = ctx.run_ui(RawInput::default(), |ui| {
        height = crate::widgets::action_button(ui, "Open").rect.height();
    });
    output.drop_without_applying_deltas();
    assert!((height - 30.0).abs() <= 0.5, "button height {height}");
}

#[test]
fn both_cards_draw_the_button_at_the_action_height() {
    for (folder, label) in [
        (None, "Open Folder..."),
        (Some(PathBuf::from("/images")), "Open Another Folder..."),
    ] {
        let ctx = themed();
        let mut gallery = Gallery::new(SortMode::Natural);
        gallery.set_folder(folder);
        let found = texts(&ctx, &mut gallery, Vec::new());
        let (_, rect) = found
            .iter()
            .find(|(text, _)| text == label)
            .expect("the button is painted");
        let height = click_target_height(&ctx, rect.center());
        assert!((height - 30.0).abs() <= 0.5, "{label}: height {height}");
    }
}

#[test]
fn every_card_is_centred_in_its_area() {
    let area = egui::Rect::from_min_size(egui::pos2(0.0, 100.0), egui::vec2(1200.0, 500.0));
    for kind in [
        EmptyKind::NoFolder,
        EmptyKind::EmptyFolder,
        EmptyKind::NoResult,
    ] {
        let ctx = themed();
        let mut gallery = Gallery::new(SortMode::Natural);
        let input = RawInput {
            screen_rect: Some(screen()),
            ..Default::default()
        };
        let output = ctx.run_ui(input, |ui| {
            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(area));
            gallery.show_empty(&mut child, kind);
        });
        let mut top = f32::MAX;
        let mut bottom = f32::MIN;
        for clipped in &output.shapes {
            if let egui::epaint::Shape::Text(shape) = &clipped.shape {
                let rect = shape.visual_bounding_rect();
                top = top.min(rect.top());
                bottom = bottom.max(rect.bottom());
            }
        }
        output.drop_without_applying_deltas();
        // The icon, 44 points high, sits one gap above the first text.
        let centre = f32::midpoint(top - 44.0 - crate::tokens::SPACING_M, bottom);
        assert!(
            (centre - area.center().y).abs() < 6.0,
            "{kind:?}: card centre {centre}, area centre {}",
            area.center().y
        );
    }
}

#[test]
fn the_card_replaces_the_painted_tiles() {
    let ctx = themed();
    let mut gallery = filled();
    frame(&ctx, &mut gallery, Vec::new()).drop_without_applying_deltas();
    assert_eq!(gallery.painted.len(), 4, "the grid painted four tiles");
    search(&mut gallery, "zzz");
    frame(&ctx, &mut gallery, Vec::new()).drop_without_applying_deltas();
    assert_eq!(gallery.painted.len(), 0, "the card left stale tiles");
}
