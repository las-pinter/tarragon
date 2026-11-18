//! Tests for the tree row widget.

use super::*;
use egui::{Context, Event, Modifiers, PointerButton, Pos2, RawInput, Shape};
use std::path::PathBuf;

/// A row with the given depth and caret.
fn row(depth: usize, has_caret: bool) -> TreeRow {
    TreeRow {
        depth,
        path: PathBuf::from("/r/a"),
        name: "a".to_owned(),
        has_caret,
        expanded: false,
    }
}

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
    actions: Vec<RowAction>,
    /// The painted shapes.
    shapes: Vec<Shape>,
    /// The rect of each row.
    rects: Vec<Rect>,
    /// The width the ui offered before the first row.
    avail: f32,
}

/// Run one frame with the rows as salt 0, 1, and so on.
fn frame(ctx: &Context, raw: RawInput, rows: &[TreeRow], selected: bool) -> Frame {
    let (mut actions, mut rects, mut avail) = (Vec::new(), Vec::new(), 0.0);
    let output = ctx.run_ui(raw, |ui| {
        avail = ui.available_width();
        for (index, row) in rows.iter().enumerate() {
            actions.push(show_tree_row(ui, row, selected, index));
            let seen = ctx.read_response(row_id(ui, index));
            rects.push(seen.expect("row registered").rect);
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
        avail,
    }
}

/// Click at `pos` over three frames and return the last frame's actions.
fn click(ctx: &Context, pos: Pos2, rows: &[TreeRow]) -> Vec<RowAction> {
    let press = |pressed| Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    };
    frame(ctx, input(vec![Event::PointerMoved(pos)]), rows, false);
    frame(ctx, input(vec![press(true)]), rows, false);
    frame(ctx, input(vec![press(false)]), rows, false).actions
}

/// A context with the theme, after one frame so the rows have rects.
fn ready(rows: &[TreeRow]) -> Context {
    let ctx = Context::default();
    crate::apply_theme(&ctx);
    frame(&ctx, input(Vec::new()), rows, false);
    ctx
}

/// The first row rect after one frame.
fn first_rect(ctx: &Context, rows: &[TreeRow]) -> Rect {
    frame(ctx, input(Vec::new()), rows, false).rects[0]
}

/// Whether any painted rect has this fill.
fn has_fill(shapes: &[Shape], fill: egui::Color32) -> bool {
    shapes
        .iter()
        .any(|shape| matches!(shape, Shape::Rect(rect) if rect.fill == fill))
}

/// The left edge of the first icon body of this colour.
fn icon_x(shapes: &[Shape], color: egui::Color32) -> f32 {
    shapes
        .iter()
        .find_map(|shape| match shape {
            Shape::Rect(rect) if rect.fill == color => Some(rect.rect.min.x),
            _ => None,
        })
        .expect("the icon painted")
}

#[test]
fn a_click_in_the_name_area_navigates() {
    let rows = [row(0, true)];
    let ctx = ready(&rows);
    let rect = first_rect(&ctx, &rows);
    let actions = click(&ctx, pos2(rect.max.x - 20.0, rect.center().y), &rows);
    assert_eq!(actions, [RowAction::Navigate]);
}

#[test]
fn a_click_on_the_caret_toggles_without_navigating() {
    let rows = [row(0, true)];
    let ctx = ready(&rows);
    let rect = first_rect(&ctx, &rows);
    let pos = pos2(rect.min.x + ROW_PAD + CARET_SLOT / 2.0, rect.center().y);
    assert_eq!(click(&ctx, pos, &rows), [RowAction::Toggle]);
}

#[test]
fn a_click_just_left_of_the_slot_still_toggles() {
    let rows = [row(0, true)];
    let ctx = ready(&rows);
    let rect = first_rect(&ctx, &rows);
    let pos = pos2(rect.min.x + ROW_PAD - 2.0, rect.center().y);
    assert_eq!(click(&ctx, pos, &rows), [RowAction::Toggle]);
}

#[test]
fn a_click_on_a_leaf_slot_navigates() {
    let rows = [row(0, false)];
    let ctx = ready(&rows);
    let rect = first_rect(&ctx, &rows);
    let pos = pos2(rect.min.x + ROW_PAD + CARET_SLOT / 2.0, rect.center().y);
    assert_eq!(click(&ctx, pos, &rows), [RowAction::Navigate]);
}

#[test]
fn rows_with_the_same_path_act_alone() {
    let rows = [row(0, false), row(0, false)];
    let ctx = ready(&rows);
    let rect = frame(&ctx, input(Vec::new()), &rows, false).rects[1];
    let actions = click(&ctx, rect.center(), &rows);
    assert_eq!(actions, [RowAction::None, RowAction::Navigate]);
}

#[test]
fn a_leaf_paints_no_caret() {
    let ctx = ready(&[]);
    let paths = |has_caret| {
        let shapes = frame(&ctx, input(Vec::new()), &[row(0, has_caret)], false).shapes;
        shapes
            .iter()
            .filter(|shape| matches!(shape, Shape::Path(_)))
            .count()
    };
    assert_eq!((paths(false), paths(true)), (0, 1));
}

#[test]
fn the_selected_row_paints_the_tertiary_fill() {
    let ctx = ready(&[]);
    let fills = |selected| {
        let shapes = frame(&ctx, input(Vec::new()), &[row(0, false)], selected).shapes;
        has_fill(&shapes, tokens::BG_TERTIARY)
    };
    assert_eq!((fills(false), fills(true)), (false, true));
}

#[test]
fn the_selected_row_paints_a_coral_icon() {
    let ctx = ready(&[]);
    let shapes = frame(&ctx, input(Vec::new()), &[row(0, false)], true).shapes;
    assert!(has_fill(&shapes, tokens::CORAL_STRONG));
}

#[test]
fn a_hovered_row_paints_the_hover_fill() {
    let rows = [row(0, false)];
    let ctx = ready(&rows);
    let pos = first_rect(&ctx, &rows).center();
    frame(&ctx, input(vec![Event::PointerMoved(pos)]), &rows, false);
    let shapes = frame(&ctx, input(Vec::new()), &rows, false).shapes;
    assert!(has_fill(&shapes, tokens::SURFACE_HOVER));
}

#[test]
fn the_icon_moves_right_with_depth() {
    let ctx = ready(&[]);
    let at = |depth| {
        let shapes = frame(&ctx, input(Vec::new()), &[row(depth, false)], false).shapes;
        icon_x(&shapes, tokens::TEXT_SECONDARY)
    };
    assert!((at(2) - at(0) - 2.0 * INDENT_STEP).abs() < f32::EPSILON);
}

#[test]
fn a_leaf_and_a_caret_row_share_the_icon_x() {
    let ctx = ready(&[]);
    let at = |has_caret| {
        let shapes = frame(&ctx, input(Vec::new()), &[row(1, has_caret)], false).shapes;
        icon_x(&shapes, tokens::TEXT_SECONDARY)
    };
    assert!((at(true) - at(false)).abs() < f32::EPSILON);
}

#[test]
fn a_row_is_25_tall_and_full_width() {
    let ctx = ready(&[]);
    let drawn = frame(&ctx, input(Vec::new()), &[row(0, true)], false);
    assert!((drawn.rects[0].height() - 25.0).abs() < f32::EPSILON);
    assert!((drawn.rects[0].width() - drawn.avail).abs() < f32::EPSILON);
}

#[test]
fn a_long_name_in_a_narrow_frame_does_not_panic() {
    let ctx = ready(&[]);
    let mut long = row(3, true);
    long.name = "a very long folder name that cannot fit".repeat(3);
    let raw = RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(20.0, 100.0))),
        ..Default::default()
    };
    let drawn = frame(&ctx, raw, &[long], true);
    assert_eq!(drawn.actions, [RowAction::None]);
}
