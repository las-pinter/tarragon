//! Tests for the open request that a double-click or `open_selected` queues.

use super::*;
use crate::grid::CellClick;
use crate::test_support;

/// A gallery with the four test items drawn twice, so the tiles have rects.
fn drawn_gallery() -> (egui::Context, Gallery) {
    let ctx = test_support::context();
    crate::theme::apply_theme(&ctx);
    let mut gallery = Gallery::new(SortMode::Date);
    gallery.set_folder(Some(PathBuf::from("/images")));
    gallery.set_items(items());
    for _ in 0..2 {
        frame(&ctx, &mut gallery, Vec::new()).drop_without_applying_deltas();
    }
    (ctx, gallery)
}

/// The centre of the tile at display `position`.
fn tile_center(ctx: &egui::Context, gallery: &Gallery, position: usize) -> Pos2 {
    let index = gallery.visible[position];
    ctx.read_response(Id::new(("tarragon-grid-tile", index)))
        .expect("the tile was painted")
        .rect
        .center()
}

/// Press and release the primary button at `at`, one frame for each event.
fn press_release(ctx: &egui::Context, gallery: &mut Gallery, at: Pos2) {
    for pressed in [true, false] {
        let button = Event::PointerButton {
            pos: at,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        frame(ctx, gallery, vec![button]).drop_without_applying_deltas();
    }
}

/// Select the tile at display `position`, adding to the selection.
fn add_to_selection(gallery: &mut Gallery, position: usize) {
    let index = gallery.visible[position];
    gallery.apply_grid_click(CellClick {
        index,
        position,
        modifiers: Modifiers::CTRL,
    });
}

fn path(name: &str) -> PathBuf {
    PathBuf::from("/images").join(name)
}

#[test]
fn a_double_click_selects_only_that_tile_and_queues_its_path_once() {
    let (ctx, mut gallery) = drawn_gallery();
    add_to_selection(&mut gallery, 0);
    add_to_selection(&mut gallery, 1);
    let at = tile_center(&ctx, &gallery, 2);
    frame(&ctx, &mut gallery, vec![Event::PointerMoved(at)]).drop_without_applying_deltas();

    press_release(&ctx, &mut gallery, at);
    press_release(&ctx, &mut gallery, at);

    // On screen under the date sort: a, b, a 2, a 10.
    assert_eq!(gallery.selected_paths(), [path("a 2.png")]);
    assert_eq!(gallery.take_open_request(), Some(path("a 2.png")));
    assert_eq!(gallery.take_open_request(), None, "the request is taken");
}

#[test]
fn a_single_click_queues_nothing() {
    let (ctx, mut gallery) = drawn_gallery();
    let at = tile_center(&ctx, &gallery, 2);
    frame(&ctx, &mut gallery, vec![Event::PointerMoved(at)]).drop_without_applying_deltas();

    press_release(&ctx, &mut gallery, at);

    assert_eq!(gallery.selected_paths(), [path("a 2.png")], "it selects");
    assert_eq!(gallery.take_open_request(), None);
}

#[test]
fn open_selected_queues_the_one_selected_path() {
    let (_ctx, mut gallery) = drawn_gallery();
    add_to_selection(&mut gallery, 1);

    gallery.open_selected();

    assert_eq!(gallery.take_open_request(), Some(path("b.png")));
}

#[test]
fn open_selected_queues_nothing_for_none_or_several() {
    let (_ctx, mut gallery) = drawn_gallery();
    gallery.open_selected();
    assert_eq!(gallery.take_open_request(), None, "none selected");

    add_to_selection(&mut gallery, 0);
    add_to_selection(&mut gallery, 1);
    gallery.open_selected();
    assert_eq!(gallery.take_open_request(), None, "two selected");
}

#[test]
fn a_double_click_on_a_tile_drops_every_other_selected_tile() {
    let (_ctx, mut gallery) = drawn_gallery();
    add_to_selection(&mut gallery, 0);
    add_to_selection(&mut gallery, 1);
    let revision = gallery.selection_revision();
    let index = gallery.visible[2];

    gallery.apply_grid_double_click(CellClick {
        index,
        position: 2,
        modifiers: Modifiers::CTRL,
    });

    assert_eq!(gallery.selected_paths(), [path("a 2.png")]);
    assert_eq!(gallery.selection_revision(), revision + 1);
    assert_eq!(gallery.take_open_request(), Some(path("a 2.png")));
}
