//! Tests that a window or an open menu owns the keyboard: the grid keys do
//! nothing and stay in the input.

use super::super::Shell;
use super::grid_keys::{assert_grid_keys_blocked, paste, path, press_one, send};
use super::keys::{frame, prefs_values, press, select_one, setup};
use crate::crash_card::CrashReport;
use crate::gallery::GridMenuEvent;
use egui::{Context, Event, Key, Modifiers, PointerButton};

#[test]
fn the_crash_card_blocks_the_grid_keys() {
    let (ctx, mut shell) = setup();
    select_one(&ctx, &mut shell, 1);
    shell.show_crash_report(CrashReport {
        file_name: "crash.txt".to_owned(),
        path: "/data/crash.txt".to_owned(),
        summary: "message".to_owned(),
    });
    frame(&ctx, &mut shell, vec![]);
    assert_grid_keys_blocked(&ctx, &mut shell);
}

#[test]
fn the_preferences_window_blocks_the_grid_keys() {
    let (ctx, mut shell) = setup();
    select_one(&ctx, &mut shell, 1);
    shell.prefs_mut().open(prefs_values(), "1.4 GB".to_owned());
    frame(&ctx, &mut shell, vec![]);
    assert_grid_keys_blocked(&ctx, &mut shell);
}

#[test]
fn the_manage_tags_window_blocks_the_grid_keys() {
    let (ctx, mut shell) = setup();
    select_one(&ctx, &mut shell, 1);
    shell.gallery_mut().open_manager();
    frame(&ctx, &mut shell, vec![]);
    assert_grid_keys_blocked(&ctx, &mut shell);
}

#[test]
fn the_rename_dialog_blocks_the_grid_keys() {
    let (ctx, mut shell) = setup();
    select_one(&ctx, &mut shell, 1);
    shell.gallery_mut().open_rename(path("b.png"));
    frame(&ctx, &mut shell, vec![]);
    frame(&ctx, &mut shell, vec![]);
    // The field holds the focus, which blocks the keys too. Let it go, so
    // only the window gate is under test.
    ctx.memory_mut(|m| {
        if let Some(id) = m.focused() {
            m.surrender_focus(id);
        }
    });
    assert_grid_keys_blocked(&ctx, &mut shell);
}

/// The rect of the tile of item `index`.
fn tile_rect(ctx: &Context, index: usize) -> egui::Rect {
    ctx.read_response(crate::grid::tile_id(index))
        .expect("the tile was painted")
        .rect
}

/// Right-click the tile of item `index`, then run one frame to draw the menu.
fn right_click_tile(ctx: &Context, shell: &mut Shell, index: usize) {
    right_click_at(ctx, shell, tile_rect(ctx, index).center());
}

/// Right-click at `at`, then run one frame to draw the menu.
fn right_click_at(ctx: &Context, shell: &mut Shell, at: egui::Pos2) {
    let button = |pressed| Event::PointerButton {
        pos: at,
        button: PointerButton::Secondary,
        pressed,
        modifiers: Modifiers::NONE,
    };
    for events in [
        vec![Event::PointerMoved(at)],
        vec![button(true)],
        vec![button(false)],
        vec![],
    ] {
        frame(ctx, shell, events);
    }
}

#[test]
fn an_open_tile_menu_keeps_all_five_keys() {
    let (ctx, mut shell) = setup();
    right_click_tile(&ctx, &mut shell, 1);
    assert_eq!(shell.selected_paths().len(), 1, "the right-click selected");

    assert_grid_keys_blocked(&ctx, &mut shell);

    // The keys did not close the menu: Escape still reaches it.
    frame(&ctx, &mut shell, vec![press(Key::Escape, Modifiers::NONE)]);
    let (events, _) = press_one(&ctx, &mut shell, Key::Delete, Modifiers::NONE);
    assert_eq!(events, vec![GridMenuEvent::Delete(vec![path("b.png")])]);
}

#[test]
fn an_open_empty_space_menu_keeps_paste() {
    let (ctx, mut shell) = setup();
    let bottom = (0..4)
        .map(|index| tile_rect(&ctx, index).max.y)
        .fold(f32::MIN, f32::max);
    right_click_at(&ctx, &mut shell, egui::pos2(300.0, bottom + 100.0));

    let (events, left) = send(&ctx, &mut shell, paste());

    assert_eq!(events, vec![], "the menu owns the keyboard");
    assert_eq!(left, [Key::V], "the press was kept");
}
