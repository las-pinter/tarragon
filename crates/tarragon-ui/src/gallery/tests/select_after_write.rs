//! Tests for selecting and scrolling after a file write.

use super::context_menu::{menu_shows, right_click, run, setup};
use super::*;

/// The `items` list under the date sort: a, b, a 2, a 10 on screen.
fn dated_gallery() -> Gallery {
    let mut gallery = Gallery::new(SortMode::Date);
    gallery.set_folder(Some(PathBuf::from("/images")));
    gallery.set_items(items());
    gallery
}

fn path(name: &str) -> PathBuf {
    PathBuf::from("/images").join(name)
}

/// A gallery of `count` files, named so that natural order is the list order.
fn long_gallery(count: usize) -> Gallery {
    let mut gallery = Gallery::new(SortMode::Natural);
    gallery.set_folder(Some(PathBuf::from("/images")));
    gallery.set_items(
        (0..count)
            .map(|number| GridItem::new(path(&format!("p{number:03}.png")), 1, None))
            .collect(),
    );
    gallery
}

/// Draw `count` quiet frames.
fn settle(ctx: &egui::Context, gallery: &mut Gallery, count: usize) {
    for _ in 0..count {
        frame(ctx, gallery, Vec::new()).drop_without_applying_deltas();
    }
}

/// Whether the last frame painted the tile of the file `name`.
fn painted(gallery: &Gallery, name: &str) -> bool {
    gallery.painted.contains(&gallery.index[&path(name)])
}

/// The list of `items` without the files in `gone`.
fn items_without(gone: &[&str]) -> Vec<GridItem> {
    items()
        .into_iter()
        .filter(|item| !gone.iter().any(|name| item.path == path(name)))
        .collect()
}

#[test]
fn select_paths_selects_the_listed_tiles_and_anchors_on_the_first() {
    let mut gallery = dated_gallery();

    gallery.select_paths(&[path("a 2.png"), path("b.png")]);

    assert_eq!(gallery.selected_paths(), [path("b.png"), path("a 2.png")]);
    assert_eq!(
        gallery.selection.anchor(),
        Some(gallery.index[&path("a 2.png")]),
        "the anchor is the first listed path"
    );
    assert_eq!(gallery.scroll_to, Some(2), "the first listed tile is asked");
}

#[test]
fn select_paths_scrolls_the_first_tile_into_view() {
    let ctx = egui::Context::default();
    crate::theme::apply_theme(&ctx);
    let mut gallery = long_gallery(200);
    settle(&ctx, &mut gallery, 2);
    assert!(!painted(&gallery, "p199.png"), "the tile starts off screen");

    gallery.select_paths(&[path("p199.png")]);
    settle(&ctx, &mut gallery, 3);

    assert!(painted(&gallery, "p199.png"), "the tile is not in view");
    assert!(!painted(&gallery, "p000.png"), "the grid did not move");
}

#[test]
fn select_paths_ignores_a_path_that_the_filter_hides() {
    let mut gallery = dated_gallery();
    search(&mut gallery, "a 2");

    gallery.select_paths(&[path("b.png"), path("a 2.png")]);

    assert_eq!(gallery.selected_paths(), [path("a 2.png")]);
    assert_eq!(
        gallery.selection.anchor(),
        Some(gallery.index[&path("a 2.png")]),
        "the first visible path anchors"
    );
}

#[test]
fn select_paths_with_no_visible_path_clears_the_selection() {
    let mut gallery = dated_gallery();
    gallery.select_paths(&[path("a.png")]);
    search(&mut gallery, "a 2");
    gallery.select_paths(&[path("a.png")]);
    assert!(gallery.selection.is_empty(), "a filtered path");
    assert_eq!(gallery.selection.anchor(), None);

    gallery.select_paths(&[path("a 2.png")]);
    gallery.select_paths(&[path("missing.png")]);

    assert_eq!(gallery.selected_paths().len(), 0, "an unknown path");
    assert_eq!(gallery.selection.anchor(), None);
    assert_eq!(gallery.scroll_to, None, "nothing to scroll to");
}

#[test]
fn select_near_after_removing_a_middle_tile_selects_its_successor() {
    let mut gallery = dated_gallery();
    // On screen: a, b, a 2, a 10.
    let removed = [path("b.png")];

    gallery.remember_place(&removed);
    gallery.set_items(items_without(&["b.png"]));
    gallery.select_near();

    assert_eq!(gallery.selected_paths(), [path("a 2.png")]);
    assert_eq!(gallery.scroll_to, Some(1));
}

#[test]
fn select_near_after_removing_the_last_tile_selects_its_predecessor() {
    let mut gallery = dated_gallery();
    let removed = [path("a 10.png")];

    gallery.remember_place(&removed);
    gallery.set_items(items_without(&["a 10.png"]));
    gallery.select_near();

    assert_eq!(gallery.selected_paths(), [path("a 2.png")]);
    assert_eq!(gallery.scroll_to, Some(2));
}

#[test]
fn select_near_with_no_tile_left_selects_nothing() {
    let mut gallery = dated_gallery();
    gallery.select_paths(&[path("a.png")]);
    let removed: Vec<PathBuf> = items().into_iter().map(|item| item.path).collect();

    gallery.remember_place(&removed);
    gallery.set_items(Vec::new());
    gallery.select_near();

    assert_eq!(gallery.selected_paths().len(), 0);
    assert_eq!(gallery.selection.anchor(), None);
    assert_eq!(gallery.scroll_to, None);
}

#[test]
fn select_near_uses_the_first_removed_tile_in_display_order() {
    let mut gallery = dated_gallery();
    // The list order is b, a, a 10, a 2, and the display order is a, b, a 2,
    // a 10. Of the removed pair, a comes first on screen.
    let removed = [path("a 2.png"), path("a.png")];

    gallery.remember_place(&removed);
    gallery.set_items(items_without(&["a 2.png", "a.png"]));
    gallery.select_near();

    assert_eq!(gallery.selected_paths(), [path("b.png")]);
}

#[test]
fn select_near_without_a_remembered_place_changes_nothing() {
    let mut gallery = dated_gallery();
    gallery.select_paths(&[path("b.png")]);
    let revision = gallery.selection_revision();

    gallery.select_near();

    assert_eq!(gallery.selected_paths(), [path("b.png")]);
    assert_eq!(gallery.selection_revision(), revision);
}

#[test]
fn select_near_uses_the_remembered_place_once() {
    let mut gallery = dated_gallery();
    gallery.remember_place(&[path("b.png")]);
    gallery.set_items(items_without(&["b.png"]));
    gallery.select_near();
    gallery.select_paths(&[path("a.png")]);

    gallery.select_near();

    assert_eq!(gallery.selected_paths(), [path("a.png")]);
}

#[test]
fn the_scroll_request_is_one_shot() {
    let ctx = egui::Context::default();
    crate::theme::apply_theme(&ctx);
    let mut gallery = long_gallery(200);
    settle(&ctx, &mut gallery, 2);
    gallery.select_paths(&[path("p199.png")]);
    settle(&ctx, &mut gallery, 1);
    assert_eq!(gallery.scroll_to, None, "the frame took the request");
    settle(&ctx, &mut gallery, 3);
    assert!(painted(&gallery, "p199.png"));

    // Wheel back to the top. A repeated request would pull the grid down.
    let over_grid = pos2(600.0, 500.0);
    let wheel = Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: vec2(0.0, 100_000.0),
        phase: egui::TouchPhase::Move,
        modifiers: Modifiers::NONE,
    };
    frame(
        &ctx,
        &mut gallery,
        vec![Event::PointerMoved(over_grid), wheel],
    )
    .drop_without_applying_deltas();
    settle(&ctx, &mut gallery, 30);

    assert!(painted(&gallery, "p000.png"), "the wheel did not scroll");
    assert!(!painted(&gallery, "p199.png"), "the request came back");
}

#[test]
fn a_view_rebuild_cancels_a_pending_scroll_request() {
    let mut gallery = dated_gallery();

    gallery.select_paths(&[path("b.png")]);
    assert_eq!(gallery.scroll_to, Some(1));
    search(&mut gallery, "a");
    assert_eq!(gallery.scroll_to, None, "a filter change");

    gallery.select_paths(&[path("a.png")]);
    gallery.set_folder(Some(PathBuf::from("/other")));
    assert_eq!(gallery.scroll_to, None, "a folder switch");
}

#[test]
fn an_empty_view_drops_a_pending_scroll_request() {
    let ctx = egui::Context::default();
    crate::theme::apply_theme(&ctx);
    let mut gallery = dated_gallery();
    gallery.select_paths(&[path("b.png")]);
    gallery.scroll_to = Some(1);
    gallery.set_items(Vec::new());
    gallery.scroll_to = Some(1);

    settle(&ctx, &mut gallery, 1);

    assert_eq!(gallery.scroll_to, None);
}

#[test]
fn a_folder_or_scope_switch_forgets_the_remembered_place() {
    let mut gallery = dated_gallery();
    gallery.remember_place(&[path("b.png")]);
    gallery.set_folder(Some(PathBuf::from("/images")));
    gallery.select_paths(&[path("a.png")]);
    gallery.select_near();
    assert_eq!(gallery.selected_paths(), [path("a.png")], "folder switch");

    gallery.remember_place(&[path("b.png")]);
    gallery.set_scope(Scope::All);
    gallery.select_near();
    assert_eq!(gallery.selected_paths(), [path("a.png")], "scope switch");
}

#[test]
fn select_paths_and_select_near_grow_the_revision_like_a_click() {
    let mut gallery = dated_gallery();
    let start = gallery.selection_revision();

    gallery.select_paths(&[path("b.png")]);
    assert_eq!(gallery.selection_revision(), start + 1, "select_paths");

    gallery.select_paths(&[path("b.png")]);
    assert_eq!(gallery.selection_revision(), start + 1, "same paths");

    gallery.remember_place(&[path("b.png")]);
    gallery.set_items(items_without(&["b.png"]));
    let before = gallery.selection_revision();
    gallery.select_near();
    assert_eq!(gallery.selection_revision(), before + 1, "select_near");
}

#[test]
fn select_paths_closes_an_open_menu() {
    let (ctx, mut gallery) = setup();
    let (_, seen) = right_click(&ctx, &mut gallery, 1);
    assert!(menu_shows(&seen), "the menu did not open");

    gallery.select_paths(&[path("a.png"), path("b.png")]);

    let after = run(&ctx, &mut gallery, Vec::new());
    assert!(!menu_shows(&after), "the menu stayed open");
}
