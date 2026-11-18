//! Tests for the Preferences window in the shell.

use super::MenuEvent;
use crate::prefs::{PrefsEvent, PrefsValues};
use crate::{Shell, test_support};
use egui::epaint::Shape;
use egui::{Context, Event, Key, Modifiers, Pos2, Rect};

fn values() -> PrefsValues {
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

fn collect(shape: &Shape, found: &mut Vec<(String, Rect)>) {
    match shape {
        Shape::Vec(shapes) => shapes.iter().for_each(|s| collect(s, found)),
        Shape::Text(t) => {
            let rect = t.galley.rect.translate(t.pos.to_vec2());
            found.push((t.galley.job.text.clone(), rect));
        }
        _ => {}
    }
}

fn frame(ctx: &Context, shell: &mut Shell, events: Vec<Event>) -> Vec<(String, Rect)> {
    let mut input = test_support::input(1400.0, events);
    input.screen_rect = Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(1400.0, 900.0)));
    let output = ctx.run_ui(input, |ui| shell.show(ui));
    let mut found = Vec::new();
    for clipped in &output.shapes {
        collect(&clipped.shape, &mut found);
    }
    output.drop_without_applying_deltas();
    found
}

fn centre_of(texts: &[(String, Rect)], want: &str) -> Pos2 {
    // A popup paints after the page, so the last match is the one on top.
    let (_, rect) = texts
        .iter()
        .rev()
        .find(|(t, _)| t == want)
        .unwrap_or_else(|| panic!("no text {want:?}"));
    rect.center()
}

fn escape() -> Event {
    Event::Key {
        key: Key::Escape,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::NONE,
    }
}

#[test]
fn choosing_preferences_in_the_settings_menu_queues_one_event() {
    let ctx = test_support::context();
    let mut shell = Shell::new();
    frame(&ctx, &mut shell, vec![]);
    let texts = frame(&ctx, &mut shell, vec![]);
    frame(
        &ctx,
        &mut shell,
        test_support::click(centre_of(&texts, "Settings")),
    );
    let texts = frame(&ctx, &mut shell, vec![]);
    frame(
        &ctx,
        &mut shell,
        test_support::click(centre_of(&texts, "Preferences...")),
    );
    assert_eq!(shell.take_menu_events(), vec![MenuEvent::OpenPreferences]);
    assert_eq!(shell.take_menu_events().len(), 0);
}

#[test]
fn choosing_open_folder_in_the_file_menu_queues_one_event() {
    let ctx = test_support::context();
    let mut shell = Shell::new();
    frame(&ctx, &mut shell, vec![]);
    let texts = frame(&ctx, &mut shell, vec![]);
    frame(
        &ctx,
        &mut shell,
        test_support::click(centre_of(&texts, "File")),
    );
    let texts = frame(&ctx, &mut shell, vec![]);
    frame(
        &ctx,
        &mut shell,
        test_support::click(centre_of(&texts, "Open Folder...")),
    );
    assert_eq!(shell.take_menu_events(), vec![MenuEvent::OpenFolder]);
}

#[test]
fn the_window_is_not_drawn_until_it_is_opened() {
    let ctx = test_support::context();
    let mut shell = Shell::new();
    frame(&ctx, &mut shell, vec![]);
    let texts = frame(&ctx, &mut shell, vec![]);
    assert!(!texts.iter().any(|(t, _)| t == "Max PSD Workers"));
    assert!(!shell.prefs_mut().is_open());
}

#[test]
fn the_shell_draws_the_window_and_hands_back_its_events() {
    let ctx = test_support::context();
    let mut shell = Shell::new();
    shell.prefs_mut().open(values(), "1.4 GB".to_owned());
    frame(&ctx, &mut shell, vec![]);
    let texts = frame(&ctx, &mut shell, vec![]);
    assert!(texts.iter().any(|(t, _)| t == "Max PSD Workers"));

    frame(&ctx, &mut shell, vec![escape()]);
    assert_eq!(shell.prefs_mut().take_events(), vec![PrefsEvent::Cancel]);
    assert!(!shell.prefs_mut().is_open());
}
