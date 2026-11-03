//! Tests that the open "Manage tags" window shields the gallery behind it.

use super::*;
use crate::test_support;

fn with_items() -> (egui::Context, Gallery) {
    let ctx = test_support::context();
    crate::theme::apply_theme(&ctx);
    let mut gallery = Gallery::new(SortMode::Natural);
    gallery.set_folder(Some(PathBuf::from("/images")));
    gallery.set_items(items());
    for _ in 0..2 {
        frame(&ctx, &mut gallery, Vec::new()).drop_without_applying_deltas();
    }
    (ctx, gallery)
}

fn click_at(ctx: &egui::Context, gallery: &mut Gallery, at: Pos2) {
    frame(ctx, gallery, test_support::click(at)).drop_without_applying_deltas();
    frame(ctx, gallery, Vec::new()).drop_without_applying_deltas();
}

/// A point left of the centred window where a click selects a tile.
fn tile_point() -> Pos2 {
    for y in (60_u16..400).step_by(10) {
        for x in (20_u16..380).step_by(10) {
            let at = Pos2::new(f32::from(x), f32::from(y));
            let (ctx, mut gallery) = with_items();
            click_at(&ctx, &mut gallery, at);
            if !gallery.selected_paths().is_empty() {
                return at;
            }
        }
    }
    panic!("no click selected a tile");
}

#[test]
fn a_click_on_a_tile_does_not_reach_it_while_the_window_is_open() {
    let at = tile_point();
    let (ctx, mut gallery) = with_items();
    gallery.tag_manager.open();
    for _ in 0..2 {
        frame(&ctx, &mut gallery, Vec::new()).drop_without_applying_deltas();
    }
    click_at(&ctx, &mut gallery, at);
    assert!(
        gallery.selected_paths().is_empty(),
        "the tile took the click"
    );
    assert!(gallery.tag_manager.is_open(), "the dim layer closed it");
}
