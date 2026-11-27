//! Tests for the favorite row widget.

use super::*;
use egui::{Color32, Context, Event, Modifiers, PointerButton, Pos2, RawInput, Rect, Shape, pos2};

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
            actions.push(show_favorite_row(ui, "same", selected, missing, false, salt, 0.0).0);
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

/// The colour of the painted star, or none when no star painted.
fn star_color(shapes: &[Shape]) -> Option<Color32> {
    shapes.iter().find_map(|shape| match shape {
        Shape::Mesh(mesh) => mesh.vertices.first().map(|vertex| vertex.color),
        _ => None,
    })
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
fn the_selected_row_paints_the_coral_deep_fill_and_light_coral_text() {
    let ctx = ready(1);
    let plain = frame(&ctx, input(Vec::new()), 1, false, false).shapes;
    let chosen = frame(&ctx, input(Vec::new()), 1, true, false).shapes;
    assert!(!has_fill(&plain, tokens::CORAL_DARK));
    assert!(has_fill(&chosen, tokens::CORAL_DARK));
    assert!(has_text_color(&chosen, tokens::CORAL_STRONG));
    assert_eq!(star_color(&chosen), Some(tokens::AMBER_ACCENT));
}

#[test]
fn a_row_paints_an_amber_star_and_primary_text() {
    let ctx = ready(1);
    let shapes = frame(&ctx, input(Vec::new()), 1, false, false).shapes;
    assert_eq!(star_color(&shapes), Some(tokens::AMBER_ACCENT));
    assert!(has_text_color(&shapes, tokens::TEXT_PRIMARY));
    assert!(
        shapes.iter().all(|shape| !matches!(shape, Shape::Path(_))),
        "no folder outline: the star stands in for it"
    );
}

#[test]
fn a_missing_row_keeps_the_text_floor() {
    let ctx = ready(1);
    let found = frame(&ctx, input(Vec::new()), 1, false, false).shapes;
    let gone = frame(&ctx, input(Vec::new()), 1, false, true).shapes;
    assert!(has_text_color(&found, tokens::TEXT_PRIMARY));
    assert!(has_text_color(&gone, tokens::TEXT_SECONDARY));
    assert!(!has_text_color(&gone, tokens::TEXT_MUTED));
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
fn a_selected_missing_row_keeps_the_fill_but_dims_the_star() {
    let ctx = ready(1);
    let shapes = frame(&ctx, input(Vec::new()), 1, true, true).shapes;
    assert!(has_fill(&shapes, tokens::CORAL_DARK));
    // The floor is too dark on the coral deep fill, so the text is lighter.
    assert!(has_text_color(&shapes, tokens::TEXT_TERTIARY));
    assert_eq!(star_color(&shapes), Some(tokens::TEXT_MUTED));
}

#[test]
fn a_hovered_missing_row_uses_the_light_text() {
    let ctx = ready(1);
    let pos = frame(&ctx, input(Vec::new()), 1, false, true).rects[0].center();
    frame(&ctx, input(vec![Event::PointerMoved(pos)]), 1, false, true);
    let shapes = frame(&ctx, input(Vec::new()), 1, false, true).shapes;
    assert!(has_text_color(&shapes, tokens::TEXT_TERTIARY));
    assert!(!has_text_color(&shapes, tokens::TEXT_SECONDARY));
}

#[test]
fn a_lifted_missing_row_uses_the_light_text() {
    let ctx = ready(1);
    let output = ctx.run_ui(input(Vec::new()), |ui| {
        paint_lifted_row(
            ui,
            "gone",
            false,
            true,
            Rect::from_min_size(Pos2::ZERO, egui::vec2(200.0, FAVORITE_HEIGHT)),
        );
    });
    let shapes: Vec<Shape> = output
        .shapes
        .iter()
        .map(|clipped| clipped.shape.clone())
        .collect();
    output.drop_without_applying_deltas();
    // The 0.75 lift dims the row when it paints; the chosen text color is the
    // light tertiary one.
    assert!(has_text_color(&shapes, tokens::TEXT_TERTIARY));
}

#[test]
fn a_missing_row_paints_a_dim_star() {
    let ctx = ready(1);
    let gone = frame(&ctx, input(Vec::new()), 1, false, true).shapes;
    assert_eq!(star_color(&gone), Some(tokens::TEXT_MUTED));
}

#[test]
fn the_row_is_34_tall_and_inset_by_the_panel_padding() {
    let ctx = ready(1);
    let (mut min_x, mut avail) = (0.0, 0.0);
    let output = ctx.run_ui(input(Vec::new()), |ui| {
        min_x = ui.max_rect().min.x;
        avail = ui.available_width();
        show_favorite_row(ui, "same", true, false, false, 0, 0.0);
    });
    let fill = output
        .shapes
        .iter()
        .find_map(|clipped| match &clipped.shape {
            Shape::Rect(rect) if rect.fill == tokens::CORAL_DARK => Some(rect.rect),
            _ => None,
        })
        .expect("the fill painted");
    output.drop_without_applying_deltas();
    assert!((fill.min.x - (min_x + 12.0)).abs() < f32::EPSILON);
    assert!((fill.width() - (avail - 24.0)).abs() < f32::EPSILON);
    assert!((fill.height() - 34.0).abs() < f32::EPSILON);
}

/// The painted shapes of two rows, the first one lifted when `lift` is set.
///
/// Like the panel, the lifted row is painted last with `paint_lifted_row`.
fn two_rows(lift: bool) -> Vec<Shape> {
    let ctx = Context::default();
    crate::apply_theme(&ctx);
    let output = ctx.run_ui(input(Vec::new()), |ui| {
        let (_, first) = show_favorite_row(ui, "same", false, false, lift, 0, 0.0);
        show_favorite_row(ui, "same", false, false, false, 1, 0.0);
        if lift {
            paint_lifted_row(ui, "same", false, false, first);
        }
    });
    let shapes = output.shapes.iter().map(|c| c.shape.clone()).collect();
    output.drop_without_applying_deltas();
    shapes
}

#[test]
fn a_lifted_row_paints_the_dimmed_highlight_and_no_border() {
    let dimmed = |color: Color32| color.gamma_multiply(0.75);
    let shapes = two_rows(true);
    assert!(has_fill(&shapes, dimmed(tokens::SURFACE_HIGHLIGHT)));
    assert!(
        !shapes
            .iter()
            .any(|s| matches!(s, Shape::LineSegment { .. }))
    );
    assert!(!has_fill(
        &two_rows(false),
        dimmed(tokens::SURFACE_HIGHLIGHT)
    ));
}

#[test]
fn the_lifted_border_is_dashed_dimmed_amber() {
    let ctx = Context::default();
    crate::apply_theme(&ctx);
    let output = ctx.run_ui(input(Vec::new()), |ui| {
        paint_lifted_border(
            ui,
            Rect::from_min_size(pos2(12.0, 10.0), vec2(100.0, FAVORITE_HEIGHT)),
        );
    });
    let dash = tokens::AMBER_ACCENT.gamma_multiply(0.75);
    let found = output
        .shapes
        .iter()
        .any(|c| matches!(&c.shape, Shape::LineSegment { stroke, .. } if stroke.color == dash));
    output.drop_without_applying_deltas();
    assert!(found);
}

#[test]
fn the_row_after_a_lifted_row_is_not_dimmed() {
    let count = |shapes: &[Shape]| {
        shapes
            .iter()
            .filter(|s| star_color(std::slice::from_ref(*s)) == Some(tokens::AMBER_ACCENT))
            .count()
    };
    let (plain, lifted) = (count(&two_rows(false)), count(&two_rows(true)));
    assert!(plain > 0);
    assert_eq!(lifted * 2, plain);
}

#[test]
fn a_lifted_row_paints_nothing_itself() {
    let ctx = Context::default();
    crate::apply_theme(&ctx);
    let output = ctx.run_ui(input(Vec::new()), |ui| {
        show_favorite_row(ui, "same", false, false, true, 0, 0.0);
    });
    let empty = output.shapes.is_empty();
    output.drop_without_applying_deltas();
    assert!(empty);
}

#[test]
fn a_shift_moves_the_row_but_not_its_place() {
    let ctx = Context::default();
    crate::apply_theme(&ctx);
    let (mut home, mut fill) = (Rect::NOTHING, None);
    let output = ctx.run_ui(input(Vec::new()), |ui| {
        home = show_favorite_row(ui, "same", true, false, false, 0, 40.0).1;
    });
    for clipped in &output.shapes {
        if let Shape::Rect(rect) = &clipped.shape
            && rect.fill == tokens::CORAL_DARK
        {
            fill = Some(rect.rect);
        }
    }
    output.drop_without_applying_deltas();
    assert_eq!(
        fill.expect("the fill painted"),
        home.translate(vec2(0.0, 40.0))
    );
}

/// Press at the centre of row 0, move down by `moved`, and return each
/// frame's action and the last cursor.
fn press_and_move(moved: f32) -> (Vec<FavoriteAction>, egui::CursorIcon) {
    let ctx = ready(1);
    let at = frame(&ctx, input(Vec::new()), 1, false, false).rects[0].center();
    let button = |pos, pressed| Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    };
    let to = at + vec2(0.0, moved);
    let steps = [
        vec![Event::PointerMoved(at)],
        vec![button(at, true)],
        vec![Event::PointerMoved(to)],
        Vec::new(),
    ];
    let (mut actions, mut cursor) = (Vec::new(), egui::CursorIcon::Default);
    for events in steps {
        let output = ctx.run_ui(input(events), |ui| {
            actions.push(show_favorite_row(ui, "same", false, false, false, 0, 0.0).0);
        });
        cursor = output.platform_output.cursor_icon;
        output.drop_without_applying_deltas();
    }
    (actions, cursor)
}

#[test]
fn a_press_that_moves_starts_a_drag_anywhere_on_the_row() {
    let (actions, cursor) = press_and_move(12.0);
    assert!(
        actions.contains(&FavoriteAction::DragStarted),
        "{actions:?}"
    );
    assert!(!actions.contains(&FavoriteAction::Open));
    assert_eq!(cursor, egui::CursorIcon::Grabbing);
}

#[test]
fn the_row_shows_no_grab_hand_at_rest() {
    let (actions, cursor) = press_and_move(0.0);
    assert!(
        !actions.contains(&FavoriteAction::DragStarted),
        "{actions:?}"
    );
    assert_ne!(cursor, egui::CursorIcon::Grab);
    assert_ne!(cursor, egui::CursorIcon::Grabbing);
}

#[test]
fn no_grip_is_painted() {
    let ctx = ready(1);
    let shapes = frame(&ctx, input(Vec::new()), 1, false, false).shapes;
    let texts: Vec<_> = shapes
        .iter()
        .filter_map(|shape| match shape {
            Shape::Text(text) => Some(text.galley.job.text.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(texts, ["same"]);
}
