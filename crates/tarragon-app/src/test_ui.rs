//! Headless frame helpers for tests that drive the gallery UI.

use eframe::egui::epaint::Shape;
use eframe::egui::{Context, Event, Modifiers, PointerButton, Pos2, RawInput, Rect, Ui, vec2};
use tarragon_ui::swatch::FILTER_SIZE;

/// What one frame painted.
pub struct Painted {
    /// Each text with its position.
    pub texts: Vec<(String, Pos2)>,
    /// How many colour swatches.
    pub swatches: usize,
}

/// Draw one frame with `events`.
pub fn frame(ctx: &Context, events: Vec<Event>, run: &mut impl FnMut(&mut Ui)) -> Painted {
    let screen = Rect::from_min_size(Pos2::ZERO, vec2(1200.0, 800.0));
    let input = RawInput {
        screen_rect: Some(screen),
        events,
        ..Default::default()
    };
    let out = ctx.run_ui(input, |ui| run(ui));
    let mut painted = Painted {
        texts: Vec::new(),
        swatches: 0,
    };
    for clipped in &out.shapes {
        match &clipped.shape {
            Shape::Text(t) => painted.texts.push((t.galley.text().to_owned(), t.pos)),
            Shape::Rect(r) if r.rect.size() == vec2(FILTER_SIZE, FILTER_SIZE) => {
                painted.swatches += 1;
            }
            _ => {}
        }
    }
    out.drop_without_applying_deltas();
    painted
}

/// Open the Manage tags window from the Filter Tags menu and return the texts
/// of the settled frame. The filter lists must hold a tag, or the menu is empty.
pub fn open_manager(ctx: &Context, run: &mut impl FnMut(&mut Ui)) -> Vec<String> {
    // A menu that is open already must not be toggled shut.
    let menu_open = frame(ctx, Vec::new(), run)
        .texts
        .iter()
        .any(|t| t.0 == "Manage tags...");
    if !menu_open {
        click_text(ctx, "Filter Tags", run);
    }
    click_text(ctx, "Manage tags...", run);
    frame(ctx, Vec::new(), run);
    let last = frame(ctx, Vec::new(), run);
    last.texts.into_iter().map(|t| t.0).collect()
}

/// Click the lowest text with this label, then draw once more. Returns the
/// texts of that last frame.
pub fn click_text(ctx: &Context, label: &str, run: &mut impl FnMut(&mut Ui)) -> Vec<String> {
    let first = frame(ctx, Vec::new(), run);
    let at = first
        .texts
        .iter()
        .filter(|t| t.0 == label)
        .max_by(|a, b| a.1.y.total_cmp(&b.1.y))
        .expect(label)
        .1
        + vec2(2.0, 2.0);
    let button = |pressed| Event::PointerButton {
        pos: at,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    };
    let events = vec![Event::PointerMoved(at), button(true), button(false)];
    frame(ctx, events, run);
    let last = frame(ctx, Vec::new(), run);
    last.texts.into_iter().map(|t| t.0).collect()
}
