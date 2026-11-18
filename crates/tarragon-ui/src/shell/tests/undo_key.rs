//! Tests for Ctrl+Z in a full Shell: it raises `Undo` only while the app holds
//! a step, and only when the focus rules let a shortcut act.

use super::super::Shell;
use super::grid_key_gates::right_click_tile;
use super::grid_keys::path;
use super::keys::{ctrl, frame, frame_held, press, search_id, select_one, setup};
use crate::gallery::GridMenuEvent;
use egui::{Context, Event, Key, Modifiers};

/// Press Z with `modifiers` held, then let go. Give the queued menu events and
/// the keys the shell left unconsumed.
fn press_z(
    ctx: &Context,
    shell: &mut Shell,
    modifiers: Modifiers,
) -> (Vec<GridMenuEvent>, Vec<Key>) {
    let left = frame_held(ctx, shell, vec![press(Key::Z, modifiers)], modifiers);
    let queued = shell.gallery_mut().take_menu_events();
    let up = Event::Key {
        key: Key::Z,
        physical_key: Some(Key::Z),
        pressed: false,
        repeat: false,
        modifiers,
    };
    // Let go of the key, or egui reads the next press as a repeat.
    frame_held(ctx, shell, vec![up], modifiers);
    (queued, left)
}

#[test]
fn ctrl_z_raises_undo_while_the_slot_is_full_and_is_kept_when_it_is_empty() {
    let (ctx, mut shell) = setup();
    select_one(&ctx, &mut shell, 1);

    let (events, left) = press_z(&ctx, &mut shell, ctrl());
    assert_eq!(events, vec![], "the slot is empty");
    assert_eq!(left, [Key::Z], "the press was kept");

    shell.gallery_mut().set_undo_available(true);
    let (events, left) = press_z(&ctx, &mut shell, ctrl());
    assert_eq!(events, vec![GridMenuEvent::Undo]);
    assert!(left.is_empty(), "the press was consumed");

    shell.gallery_mut().set_undo_available(false);
    let (events, left) = press_z(&ctx, &mut shell, ctrl());
    assert_eq!(events, vec![], "the app emptied the slot");
    assert_eq!(left, [Key::Z], "the second press was kept");
}

#[test]
fn ctrl_z_needs_no_selected_tile() {
    let (ctx, mut shell) = setup();
    shell.gallery_mut().set_undo_available(true);

    let (events, left) = press_z(&ctx, &mut shell, ctrl());

    assert_eq!(events, vec![GridMenuEvent::Undo]);
    assert!(left.is_empty(), "the press was consumed");
}

#[test]
fn ctrl_shift_z_and_a_plain_z_do_nothing() {
    let (ctx, mut shell) = setup();
    shell.gallery_mut().set_undo_available(true);
    for modifiers in [ctrl() | Modifiers::SHIFT, Modifiers::NONE, Modifiers::SHIFT] {
        let (events, left) = press_z(&ctx, &mut shell, modifiers);
        assert_eq!(events, vec![], "{modifiers:?}");
        assert_eq!(left, [Key::Z], "{modifiers:?} was kept");
    }
}

#[test]
fn a_focused_search_field_keeps_ctrl_z_for_its_own_undo() {
    let (ctx, mut shell) = setup();
    shell.gallery_mut().set_undo_available(true);
    frame(&ctx, &mut shell, vec![press(Key::F, ctrl())]);
    frame(&ctx, &mut shell, vec![]);
    assert!(ctx.memory(|m| m.has_focus(search_id())));

    let (events, _) = press_z(&ctx, &mut shell, ctrl());

    assert_eq!(events, vec![], "the field owns the key");
}

#[test]
fn an_open_tile_menu_keeps_ctrl_z_and_it_works_again_after_the_menu_closes() {
    let (ctx, mut shell) = setup();
    shell.gallery_mut().set_undo_available(true);
    right_click_tile(&ctx, &mut shell, 1);

    let (events, left) = press_z(&ctx, &mut shell, ctrl());
    assert_eq!(events, vec![], "the menu owns the keyboard");
    assert_eq!(left, [Key::Z], "the press was kept");

    frame(&ctx, &mut shell, vec![press(Key::Escape, Modifiers::NONE)]);
    let (events, _) = press_z(&ctx, &mut shell, ctrl());
    assert_eq!(events, vec![GridMenuEvent::Undo]);
}

#[test]
fn a_focused_button_and_an_open_window_keep_ctrl_z() {
    let (ctx, mut shell) = setup();
    shell.gallery_mut().set_undo_available(true);
    let tab = egui::Id::new(("tarragon-scope-tab", "All Images"));
    ctx.memory_mut(|m| m.request_focus(tab));
    frame(&ctx, &mut shell, vec![]);
    assert!(ctx.memory(|m| m.has_focus(tab)), "the tab holds focus");
    let (events, left) = press_z(&ctx, &mut shell, ctrl());
    assert_eq!(events, vec![], "a focused button");
    assert_eq!(left, [Key::Z]);

    ctx.memory_mut(|m| m.surrender_focus(tab));
    shell.gallery_mut().open_rename(path("b.png"));
    frame(&ctx, &mut shell, vec![]);
    frame(&ctx, &mut shell, vec![]);
    // The field holds the focus, which blocks the key too. Let it go, so only
    // the window gate is under test.
    ctx.memory_mut(|m| {
        if let Some(id) = m.focused() {
            m.surrender_focus(id);
        }
    });
    let (events, left) = press_z(&ctx, &mut shell, ctrl());
    assert_eq!(events, vec![], "an open window");
    assert_eq!(left, [Key::Z]);
}
