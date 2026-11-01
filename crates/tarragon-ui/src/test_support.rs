//! Shared helpers for headless widget tests.

use egui::{Context, Event, PointerButton, Pos2, RawInput, Rect, vec2};

/// Raw input for a screen `width` points wide, carrying `events`.
pub(crate) fn input(width: f32, events: Vec<Event>) -> RawInput {
    RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(width, 400.0))),
        events,
        ..Default::default()
    }
}

/// A context with the app fonts installed and one warm-up pass run.
pub(crate) fn context() -> Context {
    let ctx = Context::default();
    crate::fonts::install(&ctx);
    ctx.run_ui(input(600.0, vec![]), |_| {})
        .drop_without_applying_deltas();
    ctx
}

/// The events of a primary click at `at`: move, press, release.
pub(crate) fn click(at: Pos2) -> Vec<Event> {
    let button = |pressed| Event::PointerButton {
        pos: at,
        button: PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    vec![Event::PointerMoved(at), button(true), button(false)]
}
