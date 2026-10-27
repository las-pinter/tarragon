//! Tests for the favorite row widget.

use super::*;
use egui::{Color32, Context, Event, Modifiers, PointerButton, Pos2, RawInput, Rect, Shape};

/// Raw input for a 400 by 300 screen.
fn input(events: Vec<Event>) -> RawInput {
    RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(400.0, 300.0))),
        events,
        ..Default::default()
    }
}

/// What one frame drew and reported.
struct Frame {
    /// The action of each row.
    actions: Vec<FavoriteAction>,
    /// The painted shapes.
    shapes: Vec<Shape>,
    /// The rect of each row.
    rects: Vec<egui::Rect>,
}

/// Run one frame with the rows as salt 0, 1, and so on.
fn frame(ctx: &Context, raw: RawInput, count: usize, selected: bool, missing: bool) -> Frame {
    let (mut actions, mut rects) = (Vec::new(), Vec::new());
    let output = ctx.run_ui(raw, |ui| {
        for salt in 0..count {
            actions.push(show_favorite_row(ui, "same", selected, missing, salt));
            rects.push(ctx.read_response(row_id(ui, salt)).expect("row").rect);
        }
    });
    let shapes = output
        .shapes
        .iter()
        .map(|clipped| clipped.shape.clone())
        .collect();
    output.drop_without_applying_deltas();
    Frame {
        actions,
        shapes,
        rects,
    }
}

/// A context with the theme, after one frame so the rows have rects.
fn ready(count: usize) -> Context {
    let ctx = Context::default();
    crate::apply_theme(&ctx);
    frame(&ctx, input(Vec::new()), count, false, false);
    ctx
}

/// Whether any painted rect has this fill.
fn has_fill(shapes: &[Shape], fill: Color32) -> bool {
    shapes
        .iter()
        .any(|shape| matches!(shape, Shape::Rect(rect) if rect.fill == fill))
}

/// Whether any painted text has this colour.
fn has_text_color(shapes: &[Shape], color: Color32) -> bool {
    shapes.iter().any(|shape| match shape {
        Shape::Text(text) => text
            .galley
            .job
            .sections
            .iter()
            .any(|s| s.format.color == color),
        _ => false,
    })
}

#[test]
fn a_click_opens_the_favorite() {
    let ctx = ready(1);
    let pos = frame(&ctx, input(Vec::new()), 1, false, false).rects[0].center();
    let press = |pressed| Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    };
    frame(&ctx, input(vec![Event::PointerMoved(pos)]), 1, false, false);
    frame(&ctx, input(vec![press(true)]), 1, false, false);
    let actions = frame(&ctx, input(vec![press(false)]), 1, false, false).actions;
    assert_eq!(actions, [FavoriteAction::Open]);
}

#[test]
fn rows_with_the_same_label_act_alone() {
    let ctx = ready(2);
    let pos = frame(&ctx, input(Vec::new()), 2, false, false).rects[1].center();
    let press = |pressed| Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    };
    frame(&ctx, input(vec![Event::PointerMoved(pos)]), 2, false, false);
    frame(&ctx, input(vec![press(true)]), 2, false, false);
    let actions = frame(&ctx, input(vec![press(false)]), 2, false, false).actions;
    assert_eq!(actions, [FavoriteAction::None, FavoriteAction::Open]);
}

#[test]
fn the_selected_row_paints_the_tertiary_fill_and_coral_icon() {
    let ctx = ready(1);
    let plain = frame(&ctx, input(Vec::new()), 1, false, false).shapes;
    let chosen = frame(&ctx, input(Vec::new()), 1, true, false).shapes;
    assert!(!has_fill(&plain, tokens::BG_TERTIARY));
    assert!(has_fill(&chosen, tokens::BG_TERTIARY));
    assert!(has_fill(&chosen, tokens::CORAL_STRONG));
}

#[test]
fn a_missing_row_paints_the_dim_text_colour() {
    let ctx = ready(1);
    let found = frame(&ctx, input(Vec::new()), 1, false, false).shapes;
    let gone = frame(&ctx, input(Vec::new()), 1, false, true).shapes;
    assert!(!has_text_color(&found, tokens::TEXT_MUTED));
    assert!(has_text_color(&gone, tokens::TEXT_MUTED));
}

#[test]
fn the_empty_text_paints_the_expected_string() {
    let ctx = ready(0);
    let output = ctx.run_ui(input(Vec::new()), show_empty_favorites);
    let texts: Vec<String> = output
        .shapes
        .iter()
        .filter_map(|clipped| match &clipped.shape {
            Shape::Text(text) => Some(text.galley.job.text.clone()),
            _ => None,
        })
        .collect();
    output.drop_without_applying_deltas();
    assert_eq!(texts, ["No favorites yet."]);
}

#[test]
fn a_hovered_row_paints_the_hover_fill_and_primary_text() {
    let ctx = ready(1);
    let pos = frame(&ctx, input(Vec::new()), 1, false, false).rects[0].center();
    frame(&ctx, input(vec![Event::PointerMoved(pos)]), 1, false, false);
    let shapes = frame(&ctx, input(Vec::new()), 1, false, false).shapes;
    assert!(has_fill(&shapes, tokens::SURFACE_HOVER));
    assert!(has_text_color(&shapes, tokens::TEXT_PRIMARY));
}

#[test]
fn a_selected_missing_row_keeps_the_fill_but_dims_text_and_icon() {
    let ctx = ready(1);
    let shapes = frame(&ctx, input(Vec::new()), 1, true, true).shapes;
    assert!(has_fill(&shapes, tokens::BG_TERTIARY));
    assert!(has_text_color(&shapes, tokens::TEXT_MUTED));
    assert!(!has_fill(&shapes, tokens::CORAL_STRONG));
}

#[test]
fn a_missing_row_paints_a_dim_icon() {
    let ctx = ready(1);
    let found = frame(&ctx, input(Vec::new()), 1, false, false).shapes;
    let gone = frame(&ctx, input(Vec::new()), 1, false, true).shapes;
    assert!(!has_fill(&found, tokens::TEXT_MUTED));
    assert!(has_fill(&gone, tokens::TEXT_MUTED));
}

#[test]
fn the_fill_is_inset_6_on_each_side() {
    let ctx = ready(1);
    let (mut min_x, mut avail) = (0.0, 0.0);
    let output = ctx.run_ui(input(Vec::new()), |ui| {
        min_x = ui.max_rect().min.x;
        avail = ui.available_width();
        show_favorite_row(ui, "same", true, false, 0);
    });
    let fill = output
        .shapes
        .iter()
        .find_map(|clipped| match &clipped.shape {
            Shape::Rect(rect) if rect.fill == tokens::BG_TERTIARY => Some(rect.rect),
            _ => None,
        })
        .expect("the fill painted");
    output.drop_without_applying_deltas();
    assert!((fill.min.x - (min_x + 6.0)).abs() < f32::EPSILON);
    assert!((fill.width() - (avail - 12.0)).abs() < f32::EPSILON);
}
