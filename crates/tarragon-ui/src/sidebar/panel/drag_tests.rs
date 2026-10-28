//! Tests for the favorite drag, the drop line, and `drop_line_rect`.

use super::tests::{click, find, frame, ready};
use super::*;
use egui::{Context, Event, Key, Modifiers, PointerButton, Pos2, RawInput, Shape};

/// A panel with the favorites `fa`, `fb`, and `fc` at `/f/a`, `/f/b`, and `/f/c`.
fn three() -> Sidebar {
    let mut sidebar = Sidebar::default();
    sidebar.set_favorites(
        ["a", "b", "c"]
            .iter()
            .enumerate()
            .map(|(order, name)| FavoriteRow {
                path: PathBuf::from(format!("/f/{name}")),
                label: Some(format!("f{name}")),
                sort_order: i64::try_from(order).unwrap(),
                missing: false,
            })
            .collect(),
    );
    sidebar
}

/// The order of the favorite paths in the panel.
fn order(sidebar: &Sidebar) -> Vec<PathBuf> {
    sidebar
        .state
        .favorites()
        .iter()
        .map(|f| f.path.clone())
        .collect()
}

/// Press the grip of favorite `row`, drag to `drop_y` above or below its label, and stop before release.
fn drag_to(ctx: &Context, sidebar: &mut Sidebar, row: usize, drop_y: f32) -> Pos2 {
    let texts = frame(ctx, sidebar, Vec::new());
    let grip = texts.iter().filter(|t| t.string == "::").nth(row).unwrap();
    let start = grip.pos + vec2(2.0, 5.0);
    let end = pos2(start.x, drop_y);
    let press = |pressed, pos| Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    };
    frame(ctx, sidebar, vec![Event::PointerMoved(start)]);
    frame(ctx, sidebar, vec![press(true, start)]);
    frame(
        ctx,
        sidebar,
        vec![Event::PointerMoved(start + vec2(0.0, 8.0))],
    );
    frame(ctx, sidebar, vec![Event::PointerMoved(end)]);
    frame(ctx, sidebar, Vec::new());
    end
}

/// Release the pointer at `pos`.
fn release(ctx: &Context, sidebar: &mut Sidebar, pos: Pos2) {
    let up = Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::NONE,
    };
    frame(ctx, sidebar, vec![up]);
}

/// The y of the label `wanted`.
fn label_y(ctx: &Context, sidebar: &mut Sidebar, wanted: &str) -> f32 {
    find(&frame(ctx, sidebar, Vec::new()), wanted).pos.y
}

#[test]
fn dragging_row_2_above_row_0_queues_the_new_order() {
    let (ctx, mut sidebar) = (ready(), three());
    let top = label_y(&ctx, &mut sidebar, "fa");
    let end = drag_to(&ctx, &mut sidebar, 2, top - 3.0);
    release(&ctx, &mut sidebar, end);
    let paths = |names: [&str; 3]| names.map(|n| PathBuf::from(format!("/f/{n}"))).to_vec();
    assert_eq!(
        sidebar.take_events(),
        [SidebarEvent::Reorder(paths(["c", "a", "b"]))]
    );
}

#[test]
fn the_favorites_stay_until_set_favorites() {
    let (ctx, mut sidebar) = (ready(), three());
    let before = order(&sidebar);
    let top = label_y(&ctx, &mut sidebar, "fa");
    let end = drag_to(&ctx, &mut sidebar, 2, top - 3.0);
    release(&ctx, &mut sidebar, end);
    assert_eq!(order(&sidebar), before);
}

#[test]
fn a_drop_on_its_own_place_queues_nothing() {
    let (ctx, mut sidebar) = (ready(), three());
    let own = label_y(&ctx, &mut sidebar, "fc");
    let end = drag_to(&ctx, &mut sidebar, 2, own - 3.0);
    release(&ctx, &mut sidebar, end);
    assert_eq!(sidebar.take_events(), Vec::<SidebarEvent>::new());
}

#[test]
fn a_drop_outside_the_list_queues_nothing() {
    let (ctx, mut sidebar) = (ready(), three());
    let end = drag_to(&ctx, &mut sidebar, 0, 290.0);
    release(&ctx, &mut sidebar, end);
    assert_eq!(sidebar.take_events(), Vec::<SidebarEvent>::new());
}

#[test]
fn a_row_click_opens_and_starts_no_drag() {
    let (ctx, mut sidebar) = (ready(), three());
    click(&ctx, &mut sidebar, "fb", vec2(2.0, 5.0));
    assert_eq!(
        sidebar.take_events(),
        [SidebarEvent::Navigate(PathBuf::from("/f/b"))]
    );
    assert_eq!(sidebar.dragging, None);
}

#[test]
fn escape_cancels_the_drag() {
    let (ctx, mut sidebar) = (ready(), three());
    let top = label_y(&ctx, &mut sidebar, "fa");
    let end = drag_to(&ctx, &mut sidebar, 2, top - 3.0);
    let escape = Event::Key {
        key: Key::Escape,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::NONE,
    };
    frame(&ctx, &mut sidebar, vec![escape]);
    release(&ctx, &mut sidebar, end);
    assert_eq!(sidebar.take_events(), Vec::<SidebarEvent>::new());
}

/// Run one frame and return the 2 px amber rects, which are the drop lines.
fn drop_lines(ctx: &Context, sidebar: &mut Sidebar) -> Vec<Rect> {
    let raw = RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(400.0, 300.0))),
        ..Default::default()
    };
    let output = ctx.run_ui(raw, |ui| sidebar.show(ui));
    let lines = output
        .shapes
        .iter()
        .filter_map(|clipped| match &clipped.shape {
            Shape::Rect(r)
                if r.fill == tokens::AMBER_ACCENT && (r.rect.height() - 2.0).abs() < 0.01 =>
            {
                Some(r.rect)
            }
            _ => None,
        })
        .collect();
    output.drop_without_applying_deltas();
    lines
}

#[test]
fn the_drop_line_is_painted_amber_while_dragging() {
    let (ctx, mut sidebar) = (ready(), three());
    let top = label_y(&ctx, &mut sidebar, "fa");
    drag_to(&ctx, &mut sidebar, 2, top - 3.0);
    assert_eq!(drop_lines(&ctx, &mut sidebar).len(), 1);
}

#[test]
fn no_drop_line_is_painted_when_not_dragging() {
    let (ctx, mut sidebar) = (ready(), three());
    frame(&ctx, &mut sidebar, Vec::new());
    assert_eq!(drop_lines(&ctx, &mut sidebar).len(), 0);
}

#[test]
fn the_drop_line_is_hidden_on_a_no_op_slot() {
    let (ctx, mut sidebar) = (ready(), three());
    let own = label_y(&ctx, &mut sidebar, "fc");
    drag_to(&ctx, &mut sidebar, 2, own - 3.0);
    assert_eq!(drop_lines(&ctx, &mut sidebar).len(), 0);
}

#[test]
fn set_favorites_during_a_drag_ends_it() {
    let (ctx, mut sidebar) = (ready(), three());
    let top = label_y(&ctx, &mut sidebar, "fa");
    let end = drag_to(&ctx, &mut sidebar, 2, top - 3.0);
    let rows = sidebar.state.favorites().to_vec();
    sidebar.set_favorites(rows);
    release(&ctx, &mut sidebar, end);
    assert_eq!(sidebar.take_events(), Vec::<SidebarEvent>::new());
    assert_eq!(sidebar.dragging, None);
}

#[test]
fn a_drag_with_no_button_down_ends() {
    // A real lost release cannot be fed headless, so the state is set directly.
    let (ctx, mut sidebar) = (ready(), three());
    sidebar.dragging = Some(2);
    frame(&ctx, &mut sidebar, Vec::new());
    assert_eq!(sidebar.dragging, None);
    assert_eq!(sidebar.take_events(), Vec::<SidebarEvent>::new());
}

/// Three stacked 27 point rows from y = 10, x from 6 to 194.
fn stack() -> Vec<Rect> {
    (0..3)
        .map(|n| {
            let top = 10.0 + 27.0 * f32::from(u8::try_from(n).unwrap());
            Rect::from_min_max(pos2(6.0, top), pos2(194.0, top + 27.0))
        })
        .collect()
}

#[test]
fn the_first_drop_slot_is_centred_on_the_top_of_row_0() {
    let line = drop_line_rect(&stack(), 0).unwrap();
    assert_eq!((line.min.y, line.max.y), (9.0, 11.0));
}

#[test]
fn the_last_drop_slot_is_centred_on_the_bottom_of_the_last_row() {
    let line = drop_line_rect(&stack(), 3).unwrap();
    assert_eq!((line.min.y, line.max.y), (90.0, 92.0));
}

#[test]
fn a_middle_drop_slot_is_centred_on_the_top_of_its_row() {
    let line = drop_line_rect(&stack(), 1).unwrap();
    assert_eq!(line.center().y, 37.0);
}

#[test]
fn the_drop_line_spans_the_row_x_range() {
    let line = drop_line_rect(&stack(), 2).unwrap();
    assert_eq!((line.min.x, line.max.x), (6.0, 194.0));
}

#[test]
fn an_empty_list_has_no_drop_line() {
    assert_eq!(drop_line_rect(&[], 0), None);
}

#[test]
fn a_slot_past_the_end_has_no_drop_line() {
    assert_eq!(drop_line_rect(&stack(), 4), None);
}

/// Run one frame and return the painted shapes in paint order.
fn shapes_in_order(ctx: &Context, sidebar: &mut Sidebar) -> Vec<Shape> {
    let raw = RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(400.0, 300.0))),
        ..Default::default()
    };
    let output = ctx.run_ui(raw, |ui| sidebar.show(ui));
    let shapes = output.shapes.iter().map(|c| c.shape.clone()).collect();
    output.drop_without_applying_deltas();
    shapes
}

#[test]
fn the_lifted_border_is_painted_after_the_selected_row_fill() {
    for lifted in [0, 2] {
        let (ctx, mut sidebar) = (ready(), three());
        sidebar.set_open_folder(Some(PathBuf::from("/f/b")));
        let top = label_y(&ctx, &mut sidebar, "fa");
        drag_to(&ctx, &mut sidebar, lifted, top - 3.0);
        let shapes = shapes_in_order(&ctx, &mut sidebar);
        let dash = tokens::AMBER_ACCENT.gamma_multiply(0.75);
        let dashes: Vec<(usize, Rect)> = shapes
            .iter()
            .enumerate()
            .filter_map(|(at, shape)| match shape {
                Shape::LineSegment { points, stroke } if stroke.color == dash => {
                    Some((at, Rect::from_two_pos(points[0], points[1])))
                }
                _ => None,
            })
            .collect();
        assert!(!dashes.is_empty(), "no dashed border for row {lifted}");
        for (at, shape) in shapes.iter().enumerate() {
            let Shape::Rect(fill) = shape else { continue };
            if fill.fill != tokens::BG_TERTIARY {
                continue;
            }
            for (dash_at, dash_rect) in &dashes {
                if fill.rect.intersects(dash_rect.expand(0.5)) {
                    assert!(
                        *dash_at > at,
                        "dash {dash_at} is under the selected fill {at} (lifted row {lifted})"
                    );
                }
            }
        }
    }
}
