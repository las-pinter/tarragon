//! Tests for the filter bar text actions.

use super::{ACTION_HEIGHT, Kind, PAD_X, TAGS_TEXT, action_width, text_action};
use crate::test_support::{context, input};
use crate::tokens;
use egui::epaint::{ClippedShape, Shape};
use egui::{Color32, Context, Event, Rect, Ui};

/// What one frame drew: the action rect, whether it was clicked, the shapes.
struct Drawn {
    rect: Rect,
    width: f32,
    clicked: bool,
    shapes: Vec<ClippedShape>,
}

fn frame(ctx: &Context, events: Vec<Event>, kind: Kind) -> Drawn {
    let mut seen = None;
    let output = ctx.run_ui(input(600.0, events), |ui: &mut Ui| {
        let width = action_width(ui, TAGS_TEXT, kind);
        let response = text_action(ui, TAGS_TEXT, kind);
        seen = Some((response.rect, width, response.clicked()));
    });
    let shapes = output.shapes.clone();
    output.drop_without_applying_deltas();
    let (rect, width, clicked) = seen.expect("the closure ran");
    Drawn {
        rect,
        width,
        clicked,
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

/// The text colour and rect of the action label.
fn label(shapes: &[ClippedShape]) -> (Color32, Rect) {
    flat(shapes)
        .into_iter()
        .find_map(|s| match s {
            Shape::Text(t) if t.galley.text() == TAGS_TEXT => {
                let mut color = t.galley.job.sections[0].format.color;
                if color == Color32::PLACEHOLDER {
                    color = t.fallback_color;
                }
                Some((color, Rect::from_min_size(t.pos, t.galley.size())))
            }
            _ => None,
        })
        .expect("the label is drawn")
}

/// The fills and strokes of the rects in `shapes`.
fn rects(shapes: &[ClippedShape]) -> Vec<(Color32, egui::Stroke)> {
    flat(shapes)
        .into_iter()
        .filter_map(|s| match s {
            Shape::Rect(r) => Some((r.fill, r.stroke)),
            _ => None,
        })
        .collect()
}

#[test]
fn a_menu_action_is_coral_and_clear_is_tertiary() {
    let menu = frame(&context(), vec![], Kind::Menu);
    assert_eq!(label(&menu.shapes).0, tokens::CORAL_STRONG);
    let quiet = frame(&context(), vec![], Kind::Quiet);
    assert_eq!(label(&quiet.shapes).0, tokens::TEXT_TERTIARY);
}

#[test]
fn the_action_is_chip_tall_with_the_page_side_padding() {
    let drawn = frame(&context(), vec![], Kind::Menu);
    assert_eq!(drawn.rect.height(), ACTION_HEIGHT);
    assert_eq!(
        drawn.rect.width(),
        drawn.width,
        "the measure matches the paint"
    );
    let text = label(&drawn.shapes).1;
    assert_eq!(text.min.x - drawn.rect.min.x, PAD_X);
    assert_eq!(drawn.rect.max.x - text.max.x, PAD_X);
    assert!(rects(&drawn.shapes).is_empty(), "no fill at rest");
}

#[test]
fn hover_fills_the_action_and_a_click_reports() {
    let ctx = context();
    let first = frame(&ctx, vec![], Kind::Menu);
    let at = first.rect.center();
    let hovered = frame(&ctx, vec![Event::PointerMoved(at)], Kind::Menu);
    assert_eq!(
        rects(&hovered.shapes),
        [(tokens::SURFACE_HOVER, egui::Stroke::NONE)]
    );
    let clicked = frame(&ctx, crate::test_support::click(at), Kind::Menu);
    assert!(clicked.clicked);
}

#[test]
fn a_focused_action_shows_the_coral_ring() {
    let ctx = context();
    let mut action_id = None;
    let output = ctx.run_ui(input(600.0, vec![]), |ui| {
        action_id = Some(text_action(ui, TAGS_TEXT, Kind::Quiet).id);
    });
    output.drop_without_applying_deltas();
    ctx.memory_mut(|m| m.request_focus(action_id.expect("the closure ran")));
    let drawn = frame(&ctx, vec![], Kind::Quiet);
    let ring = egui::Stroke::new(tokens::FOCUS_RING_WIDTH, tokens::CORAL_STRONG);
    assert!(rects(&drawn.shapes).contains(&(Color32::TRANSPARENT, ring)));
}
