//! Tests for the keyboard shortcuts in a full Shell.

use super::super::{MenuEvent, Shell};
use crate::crash_card::CrashReport;
use crate::grid::GridItem;
use crate::prefs::PrefsValues;
use crate::test_support;
use egui::{Context, Event, Id, Key, Modifiers, Pos2, Rect};
use std::path::PathBuf;

/// The id of the gallery search field.
pub(super) fn search_id() -> Id {
    Id::new("tarragon-gallery-search")
}

/// What the real keyboard reports for Ctrl held on Linux.
pub(super) fn ctrl() -> Modifiers {
    Modifiers::CTRL | Modifiers::COMMAND
}

/// A press of `key` with `modifiers`.
pub(super) fn press(key: Key, modifiers: Modifiers) -> Event {
    Event::Key {
        key,
        physical_key: Some(key),
        pressed: true,
        repeat: false,
        modifiers,
    }
}

/// Run one frame with `events`. Return the keys that the shell left unconsumed.
pub(super) fn frame(ctx: &Context, shell: &mut Shell, events: Vec<Event>) -> Vec<Key> {
    frame_held(ctx, shell, events, Modifiers::NONE)
}

/// Run one frame with `events` while `held` modifiers are down. A copy event
/// carries no modifiers of its own, so the frame states them.
pub(super) fn frame_held(
    ctx: &Context,
    shell: &mut Shell,
    events: Vec<Event>,
    held: Modifiers,
) -> Vec<Key> {
    let mut events = events;
    events.insert(0, Event::ModifiersChanged(held));
    frame_events(ctx, shell, events)
}

/// Run one frame with exactly `events`, so a test sets the modifier changes
/// itself. Return the keys that the shell left unconsumed.
pub(super) fn frame_events(ctx: &Context, shell: &mut Shell, events: Vec<Event>) -> Vec<Key> {
    frame_at(ctx, shell, events, None)
}

/// Run one frame as `frame_events` does, at `time` seconds when it is set.
pub(super) fn frame_at(
    ctx: &Context,
    shell: &mut Shell,
    events: Vec<Event>,
    time: Option<f64>,
) -> Vec<Key> {
    let mut input = test_support::input(1400.0, events);
    input.time = time;
    input.screen_rect = Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(1400.0, 900.0)));
    let mut left = Vec::new();
    ctx.run_ui(input, |ui| {
        shell.show(ui);
        let watched = [
            Key::O,
            Key::Comma,
            Key::F,
            Key::A,
            Key::Enter,
            Key::F2,
            Key::Delete,
            Key::Z,
        ];
        for key in watched {
            if ui.input(|input| input.key_pressed(key)) {
                left.push(key);
            }
        }
        // Copy arrives as an event and paste as the release of V. C and V stand
        // for them.
        let has = |wanted: fn(&Event) -> bool| ui.input(|input| input.events.iter().any(wanted));
        if has(|event| matches!(event, Event::Copy)) {
            left.push(Key::C);
        }
        if has(|event| {
            matches!(
                event,
                Event::Key {
                    key: Key::V,
                    pressed: false,
                    ..
                }
            )
        }) {
            left.push(Key::V);
        }
    })
    .drop_without_applying_deltas();
    left
}

/// A shell that shows four items, drawn twice.
pub(super) fn setup() -> (Context, Shell) {
    let ctx = test_support::context();
    let mut shell = Shell::new();
    shell.set_folder(Some(PathBuf::from("/images")));
    shell.set_items(
        ["a.png", "b.png", "c.png", "d.png"]
            .into_iter()
            .map(|name| GridItem::new(PathBuf::from("/images").join(name), 1, None))
            .collect(),
    );
    frame(&ctx, &mut shell, vec![]);
    frame(&ctx, &mut shell, vec![]);
    (ctx, shell)
}

/// One press of each shortcut key with Ctrl.
fn all_shortcuts() -> Vec<Event> {
    [Key::O, Key::Comma, Key::F, Key::A]
        .map(|key| press(key, ctrl()))
        .to_vec()
}

/// Assert that the four shortcuts did nothing and stayed in the input.
fn assert_shortcuts_blocked(ctx: &Context, shell: &mut Shell) {
    let left = frame(ctx, shell, all_shortcuts());
    assert_eq!(shell.take_menu_events(), vec![]);
    assert!(shell.selected_paths().is_empty(), "nothing was selected");
    assert!(!ctx.memory(|m| m.has_focus(search_id())), "search is idle");
    assert_eq!(left, [Key::O, Key::Comma, Key::F, Key::A], "keys kept");
}

#[test]
fn ctrl_o_queues_open_folder_once() {
    let (ctx, mut shell) = setup();
    let left = frame(&ctx, &mut shell, vec![press(Key::O, ctrl())]);
    assert_eq!(shell.take_menu_events(), vec![MenuEvent::OpenFolder]);
    assert!(left.is_empty(), "the press was consumed");
    frame(&ctx, &mut shell, vec![]);
    assert_eq!(shell.take_menu_events(), vec![]);
}

#[test]
fn ctrl_comma_queues_open_preferences() {
    let (ctx, mut shell) = setup();
    frame(&ctx, &mut shell, vec![press(Key::Comma, ctrl())]);
    assert_eq!(shell.take_menu_events(), vec![MenuEvent::OpenPreferences]);
}

#[test]
fn ctrl_a_selects_every_visible_item() {
    let (ctx, mut shell) = setup();
    frame(&ctx, &mut shell, vec![press(Key::A, ctrl())]);
    assert_eq!(shell.selected_paths().len(), 4);
}

#[test]
fn ctrl_f_focuses_the_search_field() {
    let (ctx, mut shell) = setup();
    assert!(!ctx.memory(|m| m.has_focus(search_id())));
    frame(&ctx, &mut shell, vec![press(Key::F, ctrl())]);
    frame(&ctx, &mut shell, vec![]);
    assert!(ctx.memory(|m| m.has_focus(search_id())));
}

#[test]
fn ctrl_a_leaves_a_focused_text_field_its_own_select_all() {
    let (ctx, mut shell) = setup();
    frame(&ctx, &mut shell, vec![press(Key::F, ctrl())]);
    frame(&ctx, &mut shell, vec![]);
    assert!(ctx.memory(|m| m.has_focus(search_id())));

    frame(&ctx, &mut shell, vec![press(Key::A, ctrl())]);

    assert!(shell.selected_paths().is_empty(), "selection is unchanged");
}

#[test]
fn ctrl_a_works_while_a_non_text_widget_has_focus() {
    let (ctx, mut shell) = setup();
    let tab = Id::new(("tarragon-scope-tab", "All Images"));
    ctx.memory_mut(|m| m.request_focus(tab));
    frame(&ctx, &mut shell, vec![]);
    assert!(ctx.memory(|m| m.has_focus(tab)), "the tab holds focus");
    assert!(!ctx.text_edit_focused());

    frame(&ctx, &mut shell, vec![press(Key::A, ctrl())]);

    assert_eq!(shell.selected_paths().len(), 4);
}

#[test]
fn the_crash_card_blocks_the_shortcuts() {
    let (ctx, mut shell) = setup();
    shell.show_crash_report(CrashReport {
        file_name: "crash.txt".to_owned(),
        path: "/data/crash.txt".to_owned(),
        summary: "message".to_owned(),
    });
    frame(&ctx, &mut shell, vec![]);
    assert_shortcuts_blocked(&ctx, &mut shell);
}

/// Preferences values for a window that only has to open.
pub(super) fn prefs_values() -> PrefsValues {
    PrefsValues {
        max_psd_workers: 3,
        max_multi_preview: 9,
        large_canvas_mp: 20.0,
        tile_grid: "3x3".to_owned(),
        color_tag_enabled: true,
        palette_size: 8,
        min_share: 0.10,
        neutral_s: 0.15,
        cache_dir: None,
        cache_format: "jpeg".to_owned(),
        clear_full_res: true,
        cache_cap_gb: 20,
        debug: false,
    }
}

#[test]
fn the_preferences_window_blocks_the_shortcuts() {
    let (ctx, mut shell) = setup();
    shell.prefs_mut().open(prefs_values(), "1.4 GB".to_owned());
    frame(&ctx, &mut shell, vec![]);
    assert_shortcuts_blocked(&ctx, &mut shell);
}

#[test]
fn the_manage_tags_window_blocks_the_shortcuts() {
    let (ctx, mut shell) = setup();
    shell.gallery_mut().open_manager();
    frame(&ctx, &mut shell, vec![]);
    assert_shortcuts_blocked(&ctx, &mut shell);
}

#[test]
fn other_chords_do_nothing() {
    let (ctx, mut shell) = setup();
    let chords = [
        Modifiers::NONE,
        Modifiers::ALT,
        ctrl() | Modifiers::SHIFT,
        ctrl() | Modifiers::ALT,
    ];
    for modifiers in chords {
        let events = [Key::O, Key::Comma, Key::F, Key::A]
            .map(|key| press(key, modifiers))
            .to_vec();
        frame(&ctx, &mut shell, events);
        frame(&ctx, &mut shell, vec![]);
        assert_eq!(shell.take_menu_events(), vec![], "{modifiers:?}");
        assert!(shell.selected_paths().is_empty(), "{modifiers:?}");
        assert!(!ctx.memory(|m| m.has_focus(search_id())), "{modifiers:?}");
    }
}

/// Click the tile of item `index` so it becomes the only selected image.
pub(super) fn select_one(ctx: &Context, shell: &mut Shell, index: usize) {
    let tile = Id::new(("tarragon-grid-tile", index));
    let at = ctx
        .read_response(tile)
        .expect("the tile was painted")
        .rect
        .center();
    frame(ctx, shell, test_support::click(at));
    frame(ctx, shell, vec![]);
    assert_eq!(shell.selected_paths().len(), 1, "one tile is selected");
}

#[test]
fn enter_queues_the_one_selected_image_and_consumes_the_press() {
    let (ctx, mut shell) = setup();
    select_one(&ctx, &mut shell, 1);

    let left = frame(&ctx, &mut shell, vec![press(Key::Enter, Modifiers::NONE)]);

    assert_eq!(
        shell.take_open_request(),
        Some(PathBuf::from("/images/b.png"))
    );
    assert!(left.is_empty(), "the press was consumed");
    assert_eq!(shell.take_open_request(), None, "the request is taken");
}

#[test]
fn enter_queues_nothing_for_no_or_two_selected_images() {
    let (ctx, mut shell) = setup();
    let left = frame(&ctx, &mut shell, vec![press(Key::Enter, Modifiers::NONE)]);
    assert_eq!(shell.take_open_request(), None, "none selected");
    assert_eq!(left, [Key::Enter], "none selected: the press was kept");

    shell.gallery_mut().select_all_visible();
    let left = frame(&ctx, &mut shell, vec![press(Key::Enter, Modifiers::NONE)]);
    assert_eq!(shell.take_open_request(), None, "four selected");
    assert_eq!(left, [Key::Enter], "four selected: the press was kept");
}

#[test]
fn enter_with_a_focused_button_queues_nothing_and_stays_for_the_button() {
    let (ctx, mut shell) = setup();
    select_one(&ctx, &mut shell, 1);
    let tab = Id::new(("tarragon-scope-tab", "All Images"));
    ctx.memory_mut(|m| m.request_focus(tab));
    frame(&ctx, &mut shell, vec![]);
    assert!(ctx.memory(|m| m.has_focus(tab)), "the tab holds focus");

    assert_enter_blocked(&ctx, &mut shell);
}

#[test]
fn enter_with_a_focused_tile_opens_the_selected_image() {
    let (ctx, mut shell) = setup();
    select_one(&ctx, &mut shell, 1);
    ctx.memory_mut(|m| m.request_focus(crate::grid::tile_id(2)));
    frame(&ctx, &mut shell, vec![]);
    assert!(ctx.memory(|m| m.has_focus(crate::grid::tile_id(2))));

    frame(&ctx, &mut shell, vec![press(Key::Enter, Modifiers::NONE)]);

    assert_eq!(
        shell.take_open_request(),
        Some(PathBuf::from("/images/b.png"))
    );
}

/// Assert that Enter queued nothing and stayed in the input.
fn assert_enter_blocked(ctx: &Context, shell: &mut Shell) {
    let left = frame(ctx, shell, vec![press(Key::Enter, Modifiers::NONE)]);
    assert_eq!(shell.take_open_request(), None, "nothing queued");
    assert_eq!(left, [Key::Enter], "the press was kept");
}

#[test]
fn enter_in_a_focused_text_field_queues_nothing() {
    let (ctx, mut shell) = setup();
    select_one(&ctx, &mut shell, 1);
    frame(&ctx, &mut shell, vec![press(Key::F, ctrl())]);
    frame(&ctx, &mut shell, vec![]);
    assert!(ctx.memory(|m| m.has_focus(search_id())));

    assert_enter_blocked(&ctx, &mut shell);
}

#[test]
fn enter_while_the_crash_card_is_open_queues_nothing() {
    let (ctx, mut shell) = setup();
    select_one(&ctx, &mut shell, 1);
    shell.show_crash_report(CrashReport {
        file_name: "crash.txt".to_owned(),
        path: "/data/crash.txt".to_owned(),
        summary: "message".to_owned(),
    });
    frame(&ctx, &mut shell, vec![]);
    assert_enter_blocked(&ctx, &mut shell);
}

#[test]
fn enter_while_the_preferences_window_is_open_queues_nothing() {
    let (ctx, mut shell) = setup();
    select_one(&ctx, &mut shell, 1);
    shell.prefs_mut().open(prefs_values(), "1.4 GB".to_owned());
    frame(&ctx, &mut shell, vec![]);
    assert_enter_blocked(&ctx, &mut shell);
}

#[test]
fn enter_while_the_manage_tags_window_is_open_queues_nothing() {
    let (ctx, mut shell) = setup();
    select_one(&ctx, &mut shell, 1);
    shell.gallery_mut().open_manager();
    frame(&ctx, &mut shell, vec![]);
    assert_enter_blocked(&ctx, &mut shell);
}

#[test]
fn shift_enter_and_ctrl_enter_queue_nothing() {
    let (ctx, mut shell) = setup();
    select_one(&ctx, &mut shell, 1);
    for modifiers in [
        Modifiers::SHIFT,
        ctrl(),
        ctrl() | Modifiers::SHIFT,
        Modifiers::ALT,
    ] {
        frame(&ctx, &mut shell, vec![press(Key::Enter, modifiers)]);
        assert_eq!(shell.take_open_request(), None, "{modifiers:?}");
    }
}
