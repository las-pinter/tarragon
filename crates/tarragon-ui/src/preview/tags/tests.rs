//! Tests for the Tags heading and the pill row.

use super::TagPill;
use crate::preview::{ColorState, Meta, Preview, PreviewEvent};
use crate::tokens;
use egui::{Color32, Context, Event, PointerButton, Pos2, Rect, vec2};

fn input(width: f32, events: Vec<Event>) -> egui::RawInput {
    egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(width, 600.0))),
        events,
        ..Default::default()
    }
}

fn pill(name: &str, state: ColorState) -> TagPill {
    TagPill {
        name: name.into(),
        state,
    }
}

pub(super) fn preview(tags: Vec<TagPill>) -> Preview {
    let mut preview = Preview::default();
    preview.set_meta(Some(Meta {
        name: "a.png".into(),
        dimensions: None,
        size: None,
        format: "PNG".into(),
    }));
    preview.set_tags(tags);
    preview
}

/// Whether a fill is the coral-dark pill background at one of the three opacities.
fn is_pill_fill(fill: Color32) -> bool {
    [1.0, 0.5, 0.3]
        .into_iter()
        .any(|o| fill == tokens::CORAL_DARK.gamma_multiply(o))
}

/// What one frame painted that the tests look at.
#[derive(Default)]
struct Painted {
    /// Pill backgrounds: the rect and fill of each coral-dark shape.
    pills: Vec<(Rect, Color32)>,
    /// Every text: the string, the colour of its first section, and its place.
    texts: Vec<(String, Color32, Pos2)>,
    /// The cursor the frame asked for.
    cursor: egui::CursorIcon,
}

/// A context with the theme fonts, after two frames so text has a real size.
fn ctx_with_fonts_at(preview: &mut Preview, width: f32) -> Context {
    let ctx = Context::default();
    crate::theme::apply_theme(&ctx);
    frame(&ctx, preview, width, vec![]);
    frame(&ctx, preview, width, vec![]);
    ctx
}

fn ctx_with_fonts(preview: &mut Preview) -> Context {
    ctx_with_fonts_at(preview, 400.0)
}

/// Run one frame inside a pane `width` points wide.
fn frame(ctx: &Context, preview: &mut Preview, width: f32, events: Vec<Event>) -> Painted {
    use egui::epaint::Shape;
    fn collect(shape: &Shape, found: &mut Painted) {
        match shape {
            Shape::Vec(shapes) => shapes.iter().for_each(|s| collect(s, found)),
            Shape::Rect(r) if is_pill_fill(r.fill) => found.pills.push((r.rect, r.fill)),
            Shape::Text(t) => {
                let color = t.galley.job.sections[0].format.color;
                found.texts.push((t.galley.job.text.clone(), color, t.pos));
            }
            _ => {}
        }
    }
    let output = ctx.run_ui(input(width, events), |ui| preview.show(ui));
    let mut found = Painted::default();
    for clipped in &output.shapes {
        collect(&clipped.shape, &mut found);
    }
    found.cursor = output.platform_output.cursor_icon;
    output.drop_without_applying_deltas();
    found
}

fn names(found: &Painted, tags: &[TagPill]) -> Vec<(Pos2, Color32)> {
    tags.iter()
        .map(|t| {
            let (_, color, pos) = found
                .texts
                .iter()
                .find(|(text, ..)| *text == t.name)
                .unwrap_or_else(|| panic!("{} is not drawn", t.name));
            (*pos, *color)
        })
        .collect()
}

#[test]
fn pills_paint_their_names_in_order_with_the_opacity_of_their_state() {
    let tags = vec![
        pill("character", ColorState::All),
        pill("blue", ColorState::Some),
        pill("todo", ColorState::None),
    ];
    let mut preview = preview(tags.clone());
    let ctx = ctx_with_fonts(&mut preview);
    let found = frame(&ctx, &mut preview, 400.0, vec![]);
    assert_eq!(found.pills.len(), 3, "{:?}", found.pills);
    let placed = names(&found, &tags);
    for pair in placed.windows(2) {
        assert!(pair[0].0.x < pair[1].0.x, "left to right");
        assert!((pair[0].0.y - pair[1].0.y).abs() < 0.01, "one row");
    }
    for (i, tag) in tags.iter().enumerate() {
        let opacity = tag.state.opacity();
        assert_eq!(
            found.pills[i].1,
            tokens::CORAL_DARK.gamma_multiply(opacity),
            "pill {i}"
        );
        assert_eq!(
            placed[i].1,
            tokens::CORAL_STRONG.gamma_multiply(opacity),
            "text {i}"
        );
    }
}

#[test]
fn the_tags_heading_stays_when_colour_tags_are_disabled() {
    let mut preview = preview(vec![]);
    let ctx = ctx_with_fonts(&mut preview);
    preview.set_colors_enabled(false);
    let found = frame(&ctx, &mut preview, 400.0, vec![]);
    assert!(found.texts.iter().any(|(t, ..)| t == "Tags"));
}

fn click_at(at: Pos2) -> Vec<Event> {
    let button = |pressed| Event::PointerButton {
        pos: at,
        button: PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    vec![Event::PointerMoved(at), button(true), button(false)]
}

/// The pill background rect after a layout frame.
fn first_pill(ctx: &Context, preview: &mut Preview) -> Rect {
    frame(ctx, preview, 400.0, vec![]).pills[0].0
}

fn cross_pos(rect: Rect) -> Pos2 {
    Pos2::new(rect.max.x - tokens::SPACING_S - 1.0, rect.center().y)
}

fn body_pos(rect: Rect) -> Pos2 {
    Pos2::new(rect.min.x + tokens::SPACING_S + 1.0, rect.center().y)
}

#[test]
fn hovering_a_pill_paints_its_x_and_leaving_removes_it() {
    let mut preview = preview(vec![pill("blue", ColorState::All)]);
    let ctx = ctx_with_fonts(&mut preview);
    let rect = first_pill(&ctx, &mut preview);
    let has_x = |found: &Painted| found.texts.iter().any(|(t, ..)| t == "x");
    assert!(!has_x(&frame(&ctx, &mut preview, 400.0, vec![])));

    let hover = vec![Event::PointerMoved(body_pos(rect))];
    frame(&ctx, &mut preview, 400.0, hover);
    assert!(has_x(&frame(&ctx, &mut preview, 400.0, vec![])));

    frame(&ctx, &mut preview, 400.0, vec![Event::PointerGone]);
    assert!(!has_x(&frame(&ctx, &mut preview, 400.0, vec![])));
}

#[test]
fn a_click_on_the_x_queues_one_removal() {
    for state in [ColorState::All, ColorState::Some] {
        let mut preview = preview(vec![pill("blue", state)]);
        let ctx = ctx_with_fonts(&mut preview);
        let rect = first_pill(&ctx, &mut preview);
        frame(&ctx, &mut preview, 400.0, click_at(cross_pos(rect)));
        assert_eq!(
            preview.take_events(),
            [PreviewEvent::TagRemoved("blue".into())],
            "{state:?}"
        );
        assert_eq!(preview.take_events(), Vec::<PreviewEvent>::new());
    }
}

#[test]
fn a_click_on_a_partial_pill_queues_one_apply() {
    let mut preview = preview(vec![pill("blue", ColorState::Some)]);
    let ctx = ctx_with_fonts(&mut preview);
    let rect = first_pill(&ctx, &mut preview);
    frame(&ctx, &mut preview, 400.0, click_at(body_pos(rect)));
    assert_eq!(
        preview.take_events(),
        [PreviewEvent::TagApplied("blue".into())]
    );
}

#[test]
fn a_click_on_a_full_pill_queues_nothing() {
    let mut preview = preview(vec![pill("blue", ColorState::All)]);
    let ctx = ctx_with_fonts(&mut preview);
    let rect = first_pill(&ctx, &mut preview);
    frame(&ctx, &mut preview, 400.0, click_at(body_pos(rect)));
    assert_eq!(preview.take_events(), Vec::<PreviewEvent>::new());
}

#[test]
fn many_pills_in_a_narrow_pane_wrap_and_stay_inside_it() {
    let tags: Vec<TagPill> = (0..12)
        .map(|i| pill(&format!("tag-number-{i}"), ColorState::All))
        .collect();
    let mut preview = preview(tags);
    // The 12 squares do not wrap, so they would set the pane's minimum width.
    preview.set_colors_enabled(false);
    let width = 160.0;
    let ctx = ctx_with_fonts_at(&mut preview, width);
    let found = frame(&ctx, &mut preview, width, vec![]);
    assert_eq!(found.pills.len(), 12);
    let mut rows: Vec<f32> = found.pills.iter().map(|(r, _)| r.min.y).collect();
    rows.dedup_by(|a, b| (*a - *b).abs() < 0.01);
    assert!(rows.len() > 1, "one row only: {rows:?}");
    for (rect, _) in &found.pills {
        assert!(rect.max.x <= width + 0.01, "{rect:?} leaves the pane");
    }
}

mod hover;
