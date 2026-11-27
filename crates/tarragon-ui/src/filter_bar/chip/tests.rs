//! Tests for the filter chip.

use super::{
    CHIP_HEIGHT, CHIP_RADIUS, CROSS, DOT, Dot, GAP, PAD_LEFT, PAD_RIGHT, cross_rect, show_chip,
};
use crate::swatch::Swatch;
use crate::test_support::{context, input};
use crate::tokens;
use egui::epaint::{ClippedShape, Shape};
use egui::{Color32, Context, CornerRadius, Event, Modifiers, PointerButton, Pos2, Rect, Ui};

/// What one frame drew: the chip rect, whether the x was clicked, the shapes.
struct Drawn {
    rect: Rect,
    removed: bool,
    shapes: Vec<ClippedShape>,
}

const LABEL: &str = "sky";

fn draw_chip(ui: &mut Ui, dot: Dot, tip: Option<&str>) -> (Rect, bool) {
    let removed = show_chip(ui, "salt", dot, LABEL, tip);
    (ui.min_rect(), removed)
}

fn frame(ctx: &Context, events: Vec<Event>, dot: Dot, tip: Option<&str>) -> Drawn {
    let mut raw = input(600.0, events);
    raw.time = Some(f64::from(u32::try_from(ctx.cumulative_pass_nr()).unwrap_or(0)) * 0.5);
    let mut seen = None;
    let output = ctx.run_ui(raw, |ui| {
        ui.style_mut().interaction.tooltip_delay = 0.0;
        seen = Some(draw_chip(ui, dot, tip));
    });
    let shapes = output.shapes.clone();
    output.drop_without_applying_deltas();
    let (rect, removed) = seen.expect("the closure ran");
    Drawn {
        rect,
        removed,
        shapes,
    }
}

fn flat(shapes: &[ClippedShape]) -> Vec<Shape> {
    fn collect(shape: &Shape, found: &mut Vec<Shape>) {
        match shape {
            Shape::Vec(v) => v.iter().for_each(|s| collect(s, found)),
            other => found.push(other.clone()),
        }
    }
    let mut found = Vec::new();
    for c in shapes {
        collect(&c.shape, &mut found);
    }
    found
}

/// The name text and its rect.
fn name_rect(shapes: &[ClippedShape]) -> Option<Rect> {
    flat(shapes).into_iter().find_map(|s| match s {
        Shape::Text(t) if t.galley.text() == LABEL => {
            Some(Rect::from_min_size(t.pos, t.galley.size()))
        }
        _ => None,
    })
}

/// The colours of the x strokes.
fn mark_colors(shapes: &[ClippedShape]) -> Vec<Color32> {
    flat(shapes)
        .into_iter()
        .filter_map(|s| match s {
            Shape::LineSegment { stroke, .. } => Some(stroke.color),
            _ => None,
        })
        .collect()
}

fn pointer(at: Pos2, click: bool) -> Vec<Event> {
    let button = |pressed| Event::PointerButton {
        pos: at,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    };
    let mut events = vec![Event::PointerMoved(at)];
    if click {
        events.extend([button(true), button(false)]);
    }
    events
}

const TAG: Dot = Dot::Fill(tokens::CORAL_STRONG);

#[test]
fn a_click_on_the_x_removes_and_a_click_on_the_name_does_not() {
    let ctx = context();
    let first = frame(&ctx, vec![], TAG, None);
    let x = cross_rect(first.rect).center();
    assert!(frame(&ctx, pointer(x, true), TAG, None).removed);
    let ctx = context();
    let first = frame(&ctx, vec![], TAG, None);
    let name = name_rect(&first.shapes)
        .expect("the name is drawn")
        .center();
    assert!(!frame(&ctx, pointer(name, true), TAG, None).removed);
}

#[test]
fn the_chip_is_a_28_point_pill_on_the_hover_step() {
    let drawn = frame(&context(), vec![], TAG, None);
    assert_eq!(drawn.rect.height(), CHIP_HEIGHT);
    assert_eq!(f32::from(CHIP_RADIUS), CHIP_HEIGHT / 2.0);
    let fill = flat(&drawn.shapes).into_iter().find_map(|s| match s {
        Shape::Rect(r) => Some((r.rect, r.fill, r.corner_radius)),
        _ => None,
    });
    assert_eq!(
        fill,
        Some((
            drawn.rect,
            tokens::SURFACE_HOVER,
            CornerRadius::same(CHIP_RADIUS)
        ))
    );
}

#[test]
fn the_dot_the_name_and_the_x_follow_the_page_spacing() {
    let drawn = frame(&context(), vec![], TAG, None);
    let dot = flat(&drawn.shapes).into_iter().find_map(|s| match s {
        Shape::Circle(c) if c.fill == tokens::CORAL_STRONG => Some((c.center, c.radius)),
        _ => None,
    });
    let left = drawn.rect.min.x;
    let (center, radius) = dot.expect("the tag dot is drawn");
    assert_eq!(radius, DOT / 2.0);
    assert_eq!(center.x, left + PAD_LEFT + DOT / 2.0);
    let name = name_rect(&drawn.shapes).expect("the name is drawn");
    assert_eq!(name.min.x, left + PAD_LEFT + DOT + GAP);
    let x = cross_rect(drawn.rect);
    assert_eq!(x.size(), egui::vec2(CROSS, CROSS));
    assert_eq!(x.min.x - name.max.x, GAP);
    assert_eq!(drawn.rect.max.x - x.max.x, PAD_RIGHT);
}

#[test]
fn a_colour_chip_shows_the_small_swatch() {
    let red = Dot::Swatch(Swatch::Bucket(tarragon_core::color::ColorBucket::Red));
    let drawn = frame(&context(), vec![], red, None);
    let filled: Vec<_> = flat(&drawn.shapes)
        .into_iter()
        .filter_map(|s| match s {
            Shape::Circle(c) => Some(c.fill),
            _ => None,
        })
        .collect();
    assert_eq!(filled, [tokens::BUCKET_RED]);
    let checker = frame(&context(), vec![], Dot::Swatch(Swatch::Transparent), None);
    let cells = flat(&checker.shapes)
        .into_iter()
        .filter(|s| matches!(s, Shape::Path(p) if p.fill == tokens::CHECKER_DARK))
        .count();
    assert_eq!(
        cells, 8,
        "the checker dot is cut from the same 4 by 4 cells"
    );
}

#[test]
fn the_x_lights_up_under_the_pointer() {
    let ctx = context();
    let first = frame(&ctx, vec![], TAG, None);
    assert_eq!(mark_colors(&first.shapes), [tokens::TEXT_TERTIARY; 2]);
    let x = cross_rect(first.rect).center();
    let hovered = frame(&ctx, pointer(x, false), TAG, None);
    assert_eq!(mark_colors(&hovered.shapes), [tokens::TEXT_PRIMARY; 2]);
    let lit = flat(&hovered.shapes)
        .into_iter()
        .any(|s| matches!(s, Shape::Circle(c) if c.fill == tokens::SURFACE_HIGHLIGHT));
    assert!(lit, "the raised step sits behind the hovered x");
}

#[test]
fn a_focused_x_shows_the_coral_ring() {
    let ctx = context();
    // The chip keys its x under the ui id with the caller's salt.
    let mut x_id = None;
    let raw = input(600.0, vec![]);
    let output = ctx.run_ui(raw, |ui| {
        x_id = Some(ui.id().with(("chip-x", "salt")));
        draw_chip(ui, TAG, None);
    });
    output.drop_without_applying_deltas();
    ctx.memory_mut(|m| m.request_focus(x_id.expect("the closure ran")));
    let drawn = frame(&ctx, vec![], TAG, None);
    let ring = flat(&drawn.shapes).into_iter().any(|s| {
        matches!(s, Shape::Circle(c) if c.stroke.color == tokens::CORAL_STRONG
            && c.stroke.width == tokens::FOCUS_RING_WIDTH)
    });
    assert!(ring);
}

#[test]
fn a_folder_chip_shows_its_tooltip_over_the_body() {
    let ctx = context();
    let tip = Some("/home/u/sketches");
    let first = frame(&ctx, vec![], TAG, tip);
    let has_tip = |d: &Drawn| {
        flat(&d.shapes)
            .iter()
            .any(|s| matches!(s, Shape::Text(t) if t.galley.text() == "/home/u/sketches"))
    };
    assert!(!has_tip(&first));
    let name = name_rect(&first.shapes)
        .expect("the name is drawn")
        .center();
    let mut drawn = frame(&ctx, pointer(name, false), TAG, tip);
    for _ in 0..3 {
        drawn = frame(&ctx, vec![], TAG, tip);
    }
    assert!(has_tip(&drawn));
}

#[test]
fn a_long_name_ends_in_an_ellipsis_and_shows_whole_in_the_tooltip() {
    let long = "a very long tag name that keeps going and going past the bar";
    let ctx = context();
    let width = 200.0;
    let draw = |ctx: &Context, events: Vec<Event>| {
        let mut raw = input(600.0, events);
        raw.time = Some(f64::from(u32::try_from(ctx.cumulative_pass_nr()).unwrap_or(0)) * 0.5);
        let mut rect = None;
        let output = ctx.run_ui(raw, |ui| {
            ui.style_mut().interaction.tooltip_delay = 0.0;
            ui.set_max_width(width);
            show_chip(ui, "salt", TAG, long, None);
            rect = Some(ui.min_rect());
        });
        let shapes = flat(&output.shapes);
        output.drop_without_applying_deltas();
        (rect.expect("the closure ran"), shapes)
    };
    let (rect, shapes) = draw(&ctx, vec![]);
    assert!(rect.width() <= width, "{rect:?}");
    let elided = shapes.iter().find_map(|s| match s {
        Shape::Text(t) => Some(t.galley.elided),
        _ => None,
    });
    assert_eq!(elided, Some(true), "the name ends in an ellipsis");
    let mut tips = Vec::new();
    draw(
        &ctx,
        vec![Event::PointerMoved(
            rect.left_center() + egui::vec2(20.0, 0.0),
        )],
    );
    for _ in 0..3 {
        tips = draw(&ctx, vec![]).1;
    }
    assert!(
        tips.iter()
            .any(|s| matches!(s, Shape::Text(t) if t.galley.text() == long))
    );
}

#[test]
fn enter_on_a_focused_x_removes_the_filter() {
    // egui keeps widget info only under its debug option, so the
    // "Remove <name>" label is not checked here; the key path is.
    let ctx = context();
    let mut x_id = None;
    let output = ctx.run_ui(input(600.0, vec![]), |ui| {
        x_id = Some(ui.id().with(("chip-x", "salt")));
        draw_chip(ui, TAG, None);
    });
    output.drop_without_applying_deltas();
    ctx.memory_mut(|m| m.request_focus(x_id.expect("the closure ran")));
    frame(&ctx, vec![], TAG, None);
    let enter = |pressed| Event::Key {
        key: egui::Key::Enter,
        physical_key: None,
        pressed,
        repeat: false,
        modifiers: Modifiers::NONE,
    };
    assert!(frame(&ctx, vec![enter(true), enter(false)], TAG, None).removed);
}
