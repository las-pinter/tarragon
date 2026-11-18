//! Headless frame helpers for tests that drive the gallery UI.

use eframe::egui::epaint::Shape;
use eframe::egui::{
    Color32, Context, Event, Modifiers, PointerButton, Pos2, RawInput, Rect, TextureId, Ui, vec2,
};
use tarragon_ui::swatch::{self, FILTER_SIZE};

/// What one frame painted.
pub struct Painted {
    /// Each text with its position.
    pub texts: Vec<(String, Pos2)>,
    /// How many colour swatches.
    pub swatches: usize,
    /// How many shapes draw a loaded texture.
    pub pictures: usize,
}

/// Draw one frame with `events`.
pub fn frame(ctx: &Context, events: Vec<Event>, run: &mut impl FnMut(&mut Ui)) -> Painted {
    frame_at(ctx, None, events, run)
}

/// Draw one frame with `events` at input `time` in seconds. With `None`, egui
/// moves its clock on by one predicted frame, so two clicks in a row fall inside
/// the double-click window. A later `time` lets a test keep them apart.
pub fn frame_at(
    ctx: &Context,
    time: Option<f64>,
    events: Vec<Event>,
    run: &mut impl FnMut(&mut Ui),
) -> Painted {
    let screen = Rect::from_min_size(Pos2::ZERO, vec2(1200.0, 800.0));
    let input = RawInput {
        screen_rect: Some(screen),
        time,
        events,
        ..Default::default()
    };
    let out = ctx.run_ui(input, |ui| run(ui));
    let mut painted = Painted {
        texts: Vec::new(),
        swatches: 0,
        pictures: 0,
    };
    for clipped in &out.shapes {
        tally(&clipped.shape, &mut painted);
    }
    out.drop_without_applying_deltas();
    painted
}

/// Count one shape into `painted`. A window groups its shapes in a list, so the
/// walk goes into lists.
fn tally(shape: &Shape, painted: &mut Painted) {
    match shape {
        Shape::Vec(shapes) => shapes.iter().for_each(|s| tally(s, painted)),
        Shape::Text(t) => painted.texts.push((t.galley.text().to_owned(), t.pos)),
        Shape::Mesh(m) if m.texture_id != TextureId::default() => painted.pictures += 1,
        Shape::Rect(r) if r.brush.is_some() => painted.pictures += 1,
        Shape::Rect(r) if r.rect.size() == vec2(FILTER_SIZE, FILTER_SIZE) => {
            painted.swatches += 1;
        }
        _ => {}
    }
}

/// The fill of each Preview colour square that one frame paints.
///
/// The transparent square paints smaller checker cells, so 11 squares match,
/// not 12.
pub fn square_fills(ctx: &Context, run: &mut impl FnMut(&mut Ui)) -> Vec<Color32> {
    fn collect(shape: &Shape, found: &mut Vec<Color32>) {
        match shape {
            Shape::Vec(shapes) => shapes.iter().for_each(|s| collect(s, found)),
            Shape::Rect(r) if r.rect.size() == vec2(swatch::SIZE, swatch::SIZE) => {
                found.push(r.fill);
            }
            _ => {}
        }
    }
    let screen = Rect::from_min_size(Pos2::ZERO, vec2(1200.0, 800.0));
    let input = RawInput {
        screen_rect: Some(screen),
        ..Default::default()
    };
    let out = ctx.run_ui(input, |ui| run(ui));
    let mut found = Vec::new();
    for clipped in &out.shapes {
        collect(&clipped.shape, &mut found);
    }
    out.drop_without_applying_deltas();
    found
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
