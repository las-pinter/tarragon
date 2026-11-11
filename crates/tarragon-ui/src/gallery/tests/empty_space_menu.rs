//! Tests for the empty-space menu.

use super::context_menu::{
    Text, assert_no_events, click_row, key, menu_shows, press_at, right_click, run, setup,
    tile_center,
};
use super::*;
use crate::grid::tile_id;
use egui::Key;

/// Whether the Paste-only menu shows in `seen`.
pub(super) fn paste_menu_shows(seen: &[Text]) -> bool {
    seen.iter().any(|(text, ..)| text == "Paste") && !menu_shows(seen)
}

/// The rect of the tile of item `index`.
fn tile_rect(ctx: &egui::Context, index: usize) -> Rect {
    ctx.read_response(tile_id(index))
        .expect("the tile registered a response")
        .rect
}

/// A point in the gap between the first two tiles of the first row.
fn gap_point(ctx: &egui::Context, gallery: &Gallery) -> Pos2 {
    let mut rects: Vec<Rect> = (0..gallery.items.len())
        .map(|index| tile_rect(ctx, index))
        .collect();
    rects.sort_by(|a, b| a.min.x.total_cmp(&b.min.x));
    let (left, right) = (rects[0], rects[1]);
    assert!(right.min.x > left.max.x, "the tiles have no gap");
    Pos2::new(f32::midpoint(left.max.x, right.min.x), left.center().y)
}

/// A point below the last row of tiles.
fn below_point(ctx: &egui::Context, gallery: &Gallery) -> Pos2 {
    let bottom = (0..gallery.items.len())
        .map(|index| tile_rect(ctx, index).max.y)
        .fold(f32::MIN, f32::max);
    Pos2::new(300.0, bottom + 100.0)
}

#[test]
fn a_right_click_in_a_gap_and_below_the_last_row_opens_the_paste_menu() {
    let (ctx, mut gallery) = setup();
    let gap = gap_point(&ctx, &gallery);
    let seen = press_at(&ctx, &mut gallery, gap, PointerButton::Secondary);
    assert!(paste_menu_shows(&seen), "the gap did not open the menu");
    // A press outside closes it, then the second spot opens it again.
    press_at(&ctx, &mut gallery, gap, PointerButton::Primary);
    let below = below_point(&ctx, &gallery);
    let seen = press_at(&ctx, &mut gallery, below, PointerButton::Secondary);
    assert!(paste_menu_shows(&seen), "the space below did not open it");
}

#[test]
fn a_right_click_on_a_tile_opens_the_tile_menu_and_never_both() {
    let (ctx, mut gallery) = setup();
    let below = below_point(&ctx, &gallery);
    let seen = press_at(&ctx, &mut gallery, below, PointerButton::Secondary);
    assert!(paste_menu_shows(&seen));
    let (_, seen) = right_click(&ctx, &mut gallery, 0);
    assert!(menu_shows(&seen), "the tile menu did not open");
    let pastes = seen.iter().filter(|(text, ..)| text == "Paste").count();
    assert_eq!(pastes, 1, "two menus show at once");
}

#[test]
fn a_right_click_one_point_inside_a_tile_edge_opens_the_tile_menu() {
    let (ctx, mut gallery) = setup();
    let edge = tile_rect(&ctx, 0).min + vec2(1.0, 1.0);
    let seen = press_at(&ctx, &mut gallery, edge, PointerButton::Secondary);
    assert!(menu_shows(&seen), "the tile menu did not open");
}

#[test]
fn a_left_click_in_a_gap_keeps_the_selection_and_opens_no_menu() {
    let (ctx, mut gallery) = setup();
    let tile = tile_center(&ctx, 0);
    press_at(&ctx, &mut gallery, tile, PointerButton::Primary);
    let selected = gallery.selected_paths();
    assert_eq!(selected.len(), 1, "the tile click did not select");
    let gap = gap_point(&ctx, &gallery);
    let seen = press_at(&ctx, &mut gallery, gap, PointerButton::Primary);
    assert_eq!(
        gallery.selected_paths(),
        selected,
        "the gap click changed it"
    );
    assert!(!menu_shows(&seen) && !paste_menu_shows(&seen));
}

#[test]
fn a_press_in_a_gap_takes_no_focus() {
    let (ctx, mut gallery) = setup();
    let gap = gap_point(&ctx, &gallery);
    press_at(&ctx, &mut gallery, gap, PointerButton::Primary);
    // In egui 0.36 a press alone does not take focus, so this assert holds
    // for either sense. The Tab loop below is the real guard.
    assert_eq!(ctx.memory(egui::Memory::focused), None, "the press");
    // No Tab stop lies under the gap.
    for _ in 0..40 {
        run(&ctx, &mut gallery, vec![key(Key::Tab)]);
        run(&ctx, &mut gallery, Vec::new());
        let stop = ctx
            .memory(egui::Memory::focused)
            .and_then(|id| ctx.read_response(id));
        assert!(
            stop.is_none_or(|response| !response.rect.contains(gap)),
            "a Tab stop covers the gap"
        );
    }
}

#[test]
fn paste_in_the_empty_space_menu_sends_paste_and_escape_sends_nothing() {
    let (ctx, mut gallery) = setup();
    let below = below_point(&ctx, &gallery);
    let seen = press_at(&ctx, &mut gallery, below, PointerButton::Secondary);
    let after = click_row(&ctx, &mut gallery, &seen, "Paste");
    assert!(!paste_menu_shows(&after), "Paste left the menu open");
    assert_eq!(gallery.take_menu_events(), [GridMenuEvent::Paste]);
    let seen = press_at(&ctx, &mut gallery, below, PointerButton::Secondary);
    assert!(paste_menu_shows(&seen));
    run(&ctx, &mut gallery, vec![key(Key::Escape)]);
    let after = run(&ctx, &mut gallery, Vec::new());
    assert!(!paste_menu_shows(&after), "Escape left the menu open");
    assert_no_events(&mut gallery, "Escape");
}

#[test]
fn the_empty_space_menu_greys_paste_in_every_image_scope_and_skips_no_folder() {
    let (ctx, mut gallery) = setup();
    gallery.set_scope(Scope::All);
    run(&ctx, &mut gallery, Vec::new());
    let below = below_point(&ctx, &gallery);
    let seen = press_at(&ctx, &mut gallery, below, PointerButton::Secondary);
    let (_, _, color) = seen
        .iter()
        .find(|(text, ..)| text == "Paste")
        .expect("the row shows");
    assert_eq!(*color, tokens::TEXT_MUTED);
    let after = click_row(&ctx, &mut gallery, &seen, "Paste");
    assert!(paste_menu_shows(&after), "a greyed row closed the menu");
    assert_no_events(&mut gallery, "greyed Paste");

    let (ctx, mut gallery) = setup();
    let below = below_point(&ctx, &gallery);
    gallery.set_scope(Scope::All);
    gallery.set_folder(None);
    run(&ctx, &mut gallery, Vec::new());
    assert_eq!(gallery.visible_len(), 4, "the grid still holds four items");
    let seen = press_at(&ctx, &mut gallery, below, PointerButton::Secondary);
    assert!(!paste_menu_shows(&seen), "the menu opened with no folder");
}
