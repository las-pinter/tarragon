use eframe::egui::Vec2;

use super::*;

#[test]
fn maximized_sets_the_flag_and_keeps_the_default_size() {
    let view = viewport_for("T", true);
    assert_eq!(view.title.as_deref(), Some("T"));
    assert_eq!(view.maximized, Some(true));
    assert_eq!(view.inner_size, Some(Vec2::new(1280.0, 800.0)));
    assert_eq!(view.position, None);
}

#[test]
fn not_maximized_sets_the_flag_off_and_keeps_the_default_size() {
    let view = viewport_for("T", false);
    assert_eq!(view.maximized, Some(false));
    assert_eq!(view.inner_size, Some(Vec2::new(1280.0, 800.0)));
    assert_eq!(view.position, None);
}

fn maximized_info(maximized: Option<bool>) -> ViewportInfo {
    ViewportInfo {
        maximized,
        ..Default::default()
    }
}

#[test]
fn a_reading_replaces_the_old_maximized_value() {
    assert_eq!(
        maximized_from(&maximized_info(Some(true)), None),
        Some(true)
    );
    assert_eq!(
        maximized_from(&maximized_info(Some(false)), Some(true)),
        Some(false)
    );
}

#[test]
fn a_frame_with_no_reading_keeps_the_old_maximized_value() {
    assert_eq!(
        maximized_from(&maximized_info(None), Some(true)),
        Some(true)
    );
    assert_eq!(maximized_from(&maximized_info(None), None), None);
}

#[test]
fn a_minimized_or_full_screen_frame_keeps_the_old_maximized_value() {
    let minimized = ViewportInfo {
        minimized: Some(true),
        maximized: Some(false),
        ..Default::default()
    };
    assert_eq!(maximized_from(&minimized, Some(true)), Some(true));
    let full = ViewportInfo {
        fullscreen: Some(true),
        maximized: Some(false),
        ..Default::default()
    };
    assert_eq!(maximized_from(&full, Some(true)), Some(true));
}

#[test]
fn no_request_gives_no_commands() {
    assert_eq!(activate_commands(false), Vec::<ViewportCommand>::new());
}

#[test]
fn a_request_unminimizes_shows_focuses_then_asks_for_attention() {
    assert_eq!(
        activate_commands(true),
        [
            ViewportCommand::Minimized(false),
            ViewportCommand::Visible(true),
            ViewportCommand::Focus,
            ViewportCommand::RequestUserAttention(UserAttentionType::Informational),
        ]
    );
}
