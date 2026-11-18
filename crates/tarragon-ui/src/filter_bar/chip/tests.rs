//! Tests for the filter chip.

use super::{PAD_Y, draw, show_chip};
use crate::test_support::{context, input};
use crate::tokens;
use egui::epaint::{ClippedShape, Shape};
use egui::{Color32, Context, Event, Modifiers, PointerButton, Pos2, Rect, TextStyle, Ui, vec2};

/// One text shape: its string, colour, and the rect it covers.
type Text = (String, Color32, Rect);

struct Frame<T> {
    value: T,
    shapes: Vec<ClippedShape>,
}

fn frame<T>(ctx: &Context, events: Vec<Event>, body: impl Fn(&mut Ui) -> T) -> Frame<T> {
    let mut value = None;
    let mut raw = input(600.0, events);
    raw.time = Some(f64::from(u32::try_from(ctx.cumulative_pass_nr()).unwrap_or(0)) * 0.5);
    let output = ctx.run_ui(raw, |ui| {
        ui.style_mut().interaction.tooltip_delay = 0.0;
        value = Some(body(ui));
    });
    let shapes = output.shapes.clone();
    output.drop_without_applying_deltas();
    Frame {
        value: value.expect("the closure ran"),
        shapes,
    }
}

fn texts(shapes: &[ClippedShape]) -> Vec<Text> {
    fn collect(shape: &Shape, found: &mut Vec<Text>) {
        match shape {
            Shape::Vec(v) => v.iter().for_each(|s| collect(s, found)),
            Shape::Text(t) => {
                let mut color = t.galley.job.sections[0].format.color;
                if color == Color32::PLACEHOLDER {
                    color = t.fallback_color;
                }
                let rect = Rect::from_min_size(t.pos, t.galley.size());
                found.push((t.galley.text().to_owned(), color, rect));
            }
            _ => {}
        }
    }
    let mut found = Vec::new();
    for c in shapes {
        collect(&c.shape, &mut found);
    }
    found
}

/// Frames with the pointer idle, then moved (and clicked) at `at(chip_rect, x_rect)`.
/// `at` gets the whole chip rect and the hit rect of the x, in that order.
fn with_pointer<T>(
    click: bool,
    at: impl Fn(Rect, Rect) -> Pos2,
    body: impl Fn(&mut Ui) -> T,
) -> Frame<T> {
    let ctx = context();
    let first = frame(&ctx, vec![], |ui| {
        let (response, cross) = draw(ui, "sky", "tip");
        (response.rect, cross)
    });
    let pos = at(first.value.0, first.value.1);
    let button = |pressed| Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    };
    let mut events = vec![Event::PointerMoved(pos)];
    if click {
        events.extend([button(true), button(false)]);
    }
    frame(&ctx, events, &body)
}

#[test]
fn a_click_on_the_x_is_true_and_on_the_label_is_false() {
    let on_x = with_pointer(
        true,
        |_, cross| cross.center(),
        |ui| show_chip(ui, "sky", "tip"),
    );
    assert!(on_x.value);
    let on_label = with_pointer(
        true,
        |r, _| r.min + vec2(10.0, 6.0),
        |ui| show_chip(ui, "sky", "tip"),
    );
    assert!(!on_label.value);
}

#[test]
fn the_response_carries_the_tooltip() {
    let ctx = context();
    let at = |ui: &mut Ui| draw(ui, "sky", "tip").0.rect;
    let first = frame(&ctx, vec![], at);
    assert!(texts(&first.shapes).iter().all(|t| t.0 != "tip"));
    let rect = first.value;
    let moved = vec![Event::PointerMoved(rect.center())];
    frame(&ctx, moved, at);
    let mut held = frame(&ctx, vec![], at);
    for _ in 0..3 {
        held = frame(&ctx, vec![], at);
    }
    assert!(texts(&held.shapes).iter().any(|t| t.0 == "tip"));
}

#[test]
fn hover_turns_the_x_strong() {
    let x_color =
        |shapes: &[ClippedShape]| texts(shapes).into_iter().find(|t| t.0 == "x").map(|t| t.1);
    let hovered = with_pointer(false, |r, _| r.center(), |ui| draw(ui, "sky", "tip").0.rect);
    assert_eq!(x_color(&hovered.shapes), Some(tokens::CORAL_STRONG));
    let idle = frame(&context(), vec![], |ui| draw(ui, "sky", "tip").0.rect);
    assert_eq!(x_color(&idle.shapes), Some(tokens::CORAL_MUTED));
}

#[test]
fn the_height_is_the_padding_plus_the_small_text() {
    let f = frame(&context(), vec![], |ui| {
        let font = TextStyle::Small.resolve(ui.style());
        (
            draw(ui, "sky", "tip").0.rect.height(),
            ui.fonts_mut(|f| {
                f.layout_no_wrap("x".to_owned(), font, Color32::WHITE)
                    .size()
                    .y
            }),
        )
    });
    assert_eq!(f.value.0, 2.0 * PAD_Y + f.value.1);
}

#[test]
fn a_long_label_stays_clear_of_the_x() {
    let label = "a very long tag name that keeps going and going";
    let f = frame(&context(), vec![], |ui| draw(ui, label, "tip").0.rect);
    let shown = texts(&f.shapes);
    let find = |s: &str| shown.iter().find(|t| t.0 == s).expect("text is drawn");
    let (name, cross) = (find(label), find("x"));
    assert!(name.2.max.x <= cross.2.min.x);
    assert_eq!(f.value.max.x - cross.2.max.x, super::PAD_X);
}
