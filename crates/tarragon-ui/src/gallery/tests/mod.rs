//! Tests for the gallery strip and the search timer.

mod debounce;
mod frames;
mod scope;
mod search;
mod selected;
mod sort;
mod thumbs;

use std::path::PathBuf;

use super::strip::sort_label;
use super::*;
use egui::{Event, Id, Modifiers, PointerButton, Pos2, RawInput, Rect, vec2};

/// Four items in one folder. Two files share one mtime, so the date tie shows.
fn items() -> Vec<GridItem> {
    let item =
        |name: &str, mtime: i64| GridItem::new(PathBuf::from("/images").join(name), mtime, None);
    vec![
        item("b.png", 10),
        item("a.png", 20),
        item("a 10.png", 5),
        item("a 2.png", 5),
    ]
}

/// Two folders. The first folder holds the [`items`] list.
fn items_in_two_folders() -> Vec<GridItem> {
    let item = |folder: &str, name: &str, mtime: i64| {
        GridItem::new(
            PathBuf::from("/library").join(folder).join(name),
            mtime,
            None,
        )
    };
    vec![
        item("sketches", "b.png", 10),
        item("sketches", "a.png", 20),
        item("renders", "c.png", 30),
        item("renders", "d.png", 40),
    ]
}

/// The `sketches` folder of [`items_in_two_folders`].
fn sketches() -> PathBuf {
    PathBuf::from("/library/sketches")
}

/// Type `text` and let the quiet window pass.
fn search(gallery: &mut Gallery, text: &str) {
    let start = Instant::now();
    gallery.search = text.to_owned();
    gallery.debouncer.arm(start);
    gallery.apply_pending_search(start + SEARCH_DEBOUNCE);
}

/// The headless screen that the frame helpers draw into.
fn screen() -> Rect {
    Rect::from_min_size(Pos2::ZERO, vec2(1200.0, 800.0))
}

/// Draw one headless frame of the gallery with the given events.
fn frame(ctx: &egui::Context, gallery: &mut Gallery, events: Vec<Event>) -> egui::FullOutput {
    let input = RawInput {
        screen_rect: Some(screen()),
        events,
        ..Default::default()
    };
    ctx.run_ui(input, |ui| gallery.show(ui))
}

/// Click the widget with `id` in a headless context.
fn click(ctx: &egui::Context, gallery: &mut Gallery, id: Id) {
    let pos = ctx
        .read_response(id)
        .expect("the widget registered a response")
        .rect
        .center();
    let press = |pressed| Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    };
    for events in [
        vec![Event::PointerMoved(pos)],
        vec![press(true)],
        vec![press(false)],
    ] {
        frame(ctx, gallery, events).drop_without_applying_deltas();
    }
}
