//! Tests for the grid keys in a full Shell: F2, Delete, Shift+Delete,
//! Ctrl+C, and Ctrl+V.

use super::super::Shell;
use super::keys::{ctrl, frame, frame_held, press, search_id, select_one, setup};
use crate::gallery::{GridMenuEvent, Scope};
use egui::{Context, Event, Key, Modifiers};
use std::path::PathBuf;

/// The keys the grid reads, as they sit in the input.
const GRID_KEYS: [Key; 4] = [Key::F2, Key::Delete, Key::C, Key::V];

/// One press of each of the five grid keys.
fn all_grid_keys() -> Vec<Event> {
    vec![
        press(Key::F2, Modifiers::NONE),
        press(Key::Delete, Modifiers::NONE),
        press(Key::Delete, Modifiers::SHIFT),
        Event::Copy,
        paste(),
    ]
}

/// The release of V with `modifiers` held. The platform layer drops the
/// press of Ctrl+V and reports only the release.
fn v_release(modifiers: Modifiers) -> Event {
    Event::Key {
        key: Key::V,
        physical_key: Some(Key::V),
        pressed: false,
        repeat: false,
        modifiers,
    }
}

/// The release that pastes: V with Ctrl held.
pub(super) fn paste() -> Event {
    v_release(ctrl())
}

/// Run one frame with `events` while Ctrl is down, as it is for a copy.
fn frame_ctrl(ctx: &Context, shell: &mut Shell, events: Vec<Event>) -> Vec<Key> {
    frame_held(ctx, shell, events, ctrl())
}

/// The path of the item `name`.
pub(super) fn path(name: &str) -> PathBuf {
    PathBuf::from("/images").join(name)
}

/// Give keyboard focus to the tile of item `index`.
fn focus_tile(ctx: &Context, shell: &mut Shell, index: usize) {
    ctx.memory_mut(|m| m.request_focus(crate::grid::tile_id(index)));
    frame(ctx, shell, vec![]);
    assert!(ctx.memory(|m| m.has_focus(crate::grid::tile_id(index))));
}

/// Send `events` in one frame and return the grid events they queued and the
/// keys left unconsumed. Left C and V stand for a copy and a paste event.
fn send_all(
    ctx: &Context,
    shell: &mut Shell,
    events: Vec<Event>,
) -> (Vec<GridMenuEvent>, Vec<Key>) {
    let releases = events
        .iter()
        .filter_map(|event| match event {
            Event::Key {
                key,
                modifiers,
                pressed: true,
                ..
            } => Some(key_release(*key, *modifiers)),
            _ => None,
        })
        .collect();
    let left = frame_ctrl(ctx, shell, events);
    let queued = shell.gallery_mut().take_menu_events();
    // Let go of the keys, or egui reads the next press of one as a repeat.
    frame(ctx, shell, releases);
    (queued, left)
}

/// The release of `key` held with `modifiers`.
fn key_release(key: Key, modifiers: Modifiers) -> Event {
    Event::Key {
        key,
        physical_key: Some(key),
        pressed: false,
        repeat: false,
        modifiers,
    }
}

/// Send one event. See `send_all`.
pub(super) fn send(
    ctx: &Context,
    shell: &mut Shell,
    event: Event,
) -> (Vec<GridMenuEvent>, Vec<Key>) {
    send_all(ctx, shell, vec![event])
}

/// Press one key. See `send_all`.
pub(super) fn press_one(
    ctx: &Context,
    shell: &mut Shell,
    key: Key,
    modifiers: Modifiers,
) -> (Vec<GridMenuEvent>, Vec<Key>) {
    send(ctx, shell, press(key, modifiers))
}

/// Assert that the five keys queued nothing and stayed in the input.
pub(super) fn assert_grid_keys_blocked(ctx: &Context, shell: &mut Shell) {
    let left = frame_ctrl(ctx, shell, all_grid_keys());
    assert_eq!(shell.gallery_mut().take_menu_events(), vec![]);
    assert_eq!(left, GRID_KEYS, "the presses were kept");
    let up = [
        key_release(Key::F2, Modifiers::NONE),
        key_release(Key::Delete, Modifiers::NONE),
        key_release(Key::Delete, Modifiers::SHIFT),
    ];
    frame(ctx, shell, up.to_vec());
}

#[test]
fn f2_raises_rename_for_one_tile_and_is_kept_for_two() {
    let (ctx, mut shell) = setup();
    select_one(&ctx, &mut shell, 1);
    focus_tile(&ctx, &mut shell, 1);

    let (events, left) = press_one(&ctx, &mut shell, Key::F2, Modifiers::NONE);
    assert_eq!(events, vec![GridMenuEvent::Rename(path("b.png"))]);
    assert!(left.is_empty(), "the press was consumed");

    shell.gallery_mut().select_all_visible();
    let (events, left) = press_one(&ctx, &mut shell, Key::F2, Modifiers::NONE);
    assert_eq!(events, vec![]);
    assert_eq!(left, [Key::F2], "the press was kept");
}

#[test]
fn delete_shift_delete_copy_and_paste_raise_their_menu_events() {
    let (ctx, mut shell) = setup();
    select_one(&ctx, &mut shell, 1);
    focus_tile(&ctx, &mut shell, 1);
    let b = vec![path("b.png")];
    let cases = [
        (
            press(Key::Delete, Modifiers::NONE),
            GridMenuEvent::Delete(b.clone()),
        ),
        (
            press(Key::Delete, Modifiers::SHIFT),
            GridMenuEvent::DeletePermanently(b.clone()),
        ),
        (Event::Copy, GridMenuEvent::Copy(b)),
        (paste(), GridMenuEvent::Paste),
    ];
    for (event, expected) in cases {
        let (events, left) = send(&ctx, &mut shell, event.clone());
        assert_eq!(events, vec![expected], "{event:?}");
        assert!(left.is_empty(), "{event:?} was consumed");
    }
}

#[test]
fn a_repeat_of_a_held_key_raises_nothing() {
    let (ctx, mut shell) = setup();
    select_one(&ctx, &mut shell, 1);
    for key in [Key::F2, Key::Delete] {
        // egui reads a press of a key that is not down as a new press, so the
        // key goes down first.
        frame(&ctx, &mut shell, vec![press(key, Modifiers::NONE)]);
        assert_eq!(shell.gallery_mut().take_menu_events().len(), 1, "{key:?}");
        let held = Event::Key {
            key,
            physical_key: Some(key),
            pressed: true,
            repeat: true,
            modifiers: Modifiers::NONE,
        };
        let left = frame(&ctx, &mut shell, vec![held]);
        assert_eq!(shell.gallery_mut().take_menu_events(), vec![], "{key:?}");
        assert_eq!(left, [key], "{key:?} was kept");
        frame(&ctx, &mut shell, vec![key_release(key, Modifiers::NONE)]);
    }
}

#[test]
fn delete_with_shift_delete_in_one_frame_raises_one_plain_delete() {
    let (ctx, mut shell) = setup();
    select_one(&ctx, &mut shell, 1);
    let plain = press(Key::Delete, Modifiers::NONE);
    let shifted = press(Key::Delete, Modifiers::SHIFT);
    // A key goes up between two presses, or egui reads the second as a repeat.
    let up = key_release(Key::Delete, Modifiers::NONE);
    for events in [
        vec![plain.clone(), up.clone(), shifted.clone()],
        vec![shifted, up, plain],
    ] {
        let (queued, _) = send_all(&ctx, &mut shell, events);
        assert_eq!(queued, vec![GridMenuEvent::Delete(vec![path("b.png")])]);
    }
}

#[test]
fn delete_and_copy_take_every_selected_tile_in_grid_order() {
    let (ctx, mut shell) = setup();
    shell.gallery_mut().select_all_visible();
    let all = ["a.png", "b.png", "c.png", "d.png"].map(path).to_vec();

    let (events, _) = press_one(&ctx, &mut shell, Key::Delete, Modifiers::NONE);
    assert_eq!(events, vec![GridMenuEvent::Delete(all.clone())]);
    let (events, _) = send(&ctx, &mut shell, Event::Copy);
    assert_eq!(events, vec![GridMenuEvent::Copy(all)]);
}

#[test]
fn a_v_release_without_exact_ctrl_and_a_lone_paste_event_do_nothing() {
    let (ctx, mut shell) = setup();
    for modifiers in [
        Modifiers::NONE,
        Modifiers::SHIFT,
        ctrl() | Modifiers::SHIFT,
        ctrl() | Modifiers::ALT,
    ] {
        let (events, _) = send(&ctx, &mut shell, v_release(modifiers));
        assert_eq!(events, vec![], "{modifiers:?}");
    }
    let (events, _) = send(&ctx, &mut shell, Event::Paste("x".to_owned()));
    assert_eq!(events, vec![], "a paste event alone does not paste");
}

#[test]
fn paste_fires_once_when_the_platform_sends_both_a_paste_event_and_the_release() {
    let (ctx, mut shell) = setup();
    let both = vec![Event::Paste("x".to_owned()), paste()];
    let (events, _) = send_all(&ctx, &mut shell, both);
    assert_eq!(events, vec![GridMenuEvent::Paste]);
}

#[test]
fn paste_needs_no_selected_tile_but_one_open_folder() {
    let (ctx, mut shell) = setup();
    let (events, left) = send(&ctx, &mut shell, paste());
    assert_eq!(events, vec![GridMenuEvent::Paste], "no tile is selected");
    assert!(left.is_empty(), "the press was consumed");

    shell.gallery_mut().set_scope(Scope::All);
    let (events, left) = send(&ctx, &mut shell, paste());
    assert_eq!(events, vec![], "the all-images scope has no folder");
    assert_eq!(left, [Key::V], "the press was kept");

    shell.gallery_mut().set_scope(Scope::Folder);
    shell.set_folder(None);
    let (events, left) = send(&ctx, &mut shell, paste());
    assert_eq!(events, vec![], "no folder is open");
    assert_eq!(left, [Key::V], "the press was kept");
}

#[test]
fn keys_that_need_a_tile_do_nothing_with_no_selection() {
    let (ctx, mut shell) = setup();
    let left = frame_ctrl(&ctx, &mut shell, all_grid_keys());
    assert_eq!(
        shell.gallery_mut().take_menu_events(),
        vec![GridMenuEvent::Paste]
    );
    assert_eq!(
        left,
        [Key::F2, Key::Delete, Key::C],
        "only Paste took its press"
    );
}

#[test]
fn extra_modifiers_do_nothing() {
    let (ctx, mut shell) = setup();
    select_one(&ctx, &mut shell, 1);
    let chords = [
        (Key::F2, Modifiers::SHIFT),
        (Key::F2, ctrl()),
        (Key::Delete, ctrl()),
        (Key::Delete, Modifiers::SHIFT | Modifiers::ALT),
    ];
    for (key, modifiers) in chords {
        let (events, _) = press_one(&ctx, &mut shell, key, modifiers);
        assert_eq!(events, vec![], "{key:?} {modifiers:?}");
    }
    // A copy event carries no modifiers. It also comes for these chords.
    for held in [ctrl() | Modifiers::SHIFT, ctrl() | Modifiers::ALT] {
        let left = frame_held(&ctx, &mut shell, vec![Event::Copy], held);
        assert_eq!(shell.gallery_mut().take_menu_events(), vec![], "{held:?}");
        assert_eq!(left, [Key::C], "{held:?} left the copy event");
    }
}

#[test]
fn a_focused_text_field_keeps_all_five_keys() {
    let (ctx, mut shell) = setup();
    select_one(&ctx, &mut shell, 1);
    frame(&ctx, &mut shell, vec![press(Key::F, ctrl())]);
    frame(&ctx, &mut shell, vec![]);
    assert!(ctx.memory(|m| m.has_focus(search_id())));

    let left = frame_ctrl(&ctx, &mut shell, all_grid_keys());

    assert_eq!(shell.gallery_mut().take_menu_events(), vec![]);
    assert_eq!(left, GRID_KEYS, "the presses were kept");
}

#[test]
fn a_focused_button_keeps_all_five_keys() {
    let (ctx, mut shell) = setup();
    select_one(&ctx, &mut shell, 1);
    let tab = egui::Id::new(("tarragon-scope-tab", "All Images"));
    ctx.memory_mut(|m| m.request_focus(tab));
    frame(&ctx, &mut shell, vec![]);
    assert!(ctx.memory(|m| m.has_focus(tab)), "the tab holds focus");

    assert_grid_keys_blocked(&ctx, &mut shell);
}
