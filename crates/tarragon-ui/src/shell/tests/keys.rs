//! Tests for the keyboard shortcuts in a full Shell.

use super::super::{MenuEvent, Shell};
use crate::crash_card::CrashReport;
use crate::grid::GridItem;
use crate::prefs::PrefsValues;
use crate::test_support;
use egui::{Context, Event, Id, Key, Modifiers, Pos2, Rect};
use std::path::PathBuf;

/// The id of the gallery search field.
fn search_id() -> Id {
    Id::new("tarragon-gallery-search")
}

/// What the real keyboard reports for Ctrl held on Linux.
fn ctrl() -> Modifiers {
    Modifiers::CTRL | Modifiers::COMMAND
}

/// A press of `key` with `modifiers`.
fn press(key: Key, modifiers: Modifiers) -> Event {
    Event::Key {
        key,
        physical_key: Some(key),
        pressed: true,
        repeat: false,
        modifiers,
    }
}

/// Run one frame with `events`. Return the keys that the shell left unconsumed.
fn frame(ctx: &Context, shell: &mut Shell, events: Vec<Event>) -> Vec<Key> {
    let mut input = test_support::input(1400.0, events);
    input.screen_rect = Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(1400.0, 900.0)));
    let mut left = Vec::new();
    ctx.run_ui(input, |ui| {
        shell.show(ui);
        for key in [Key::O, Key::Comma, Key::F, Key::A] {
            if ui.input(|input| input.key_pressed(key)) {
                left.push(key);
            }
        }
    })
    .drop_without_applying_deltas();
    left
}

/// A shell that shows four items, drawn twice.
fn setup() -> (Context, Shell) {
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

#[test]
fn the_preferences_window_blocks_the_shortcuts() {
    let (ctx, mut shell) = setup();
    let values = PrefsValues {
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
    };
    shell.prefs_mut().open(values, "1.4 GB".to_owned());
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
