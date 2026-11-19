//! Tests for the favorite drag: reorder, slide, and the lifted row.

use super::tests::{click, find, frame, ready, warm_up};
use super::*;
use egui::{
    Context, Event, Key, Modifiers, PointerButton, Pos2, RawInput, Rect, Shape, pos2, vec2,
};

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
    warm_up(ctx, sidebar);
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
    warm_up(ctx, sidebar);
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

/// Run frames with the pointer held until every slide has ended.
fn settle(ctx: &Context, sidebar: &mut Sidebar) {
    for _ in 0..20 {
        frame(ctx, sidebar, Vec::new());
    }
}

/// The y of each label in `names`, in one frame.
fn label_ys(ctx: &Context, sidebar: &mut Sidebar, names: [&str; 3]) -> [f32; 3] {
    let texts = frame(ctx, sidebar, Vec::new());
    names.map(|name| find(&texts, name).pos.y)
}

/// The distance between two favorite rows.
const PITCH: f32 = 27.0;

#[test]
fn the_rows_between_slide_down_while_a_row_is_dragged_up() {
    let (ctx, mut sidebar) = (ready(), three());
    warm_up(&ctx, &mut sidebar);
    let before = label_ys(&ctx, &mut sidebar, ["fa", "fb", "fc"]);
    drag_to(&ctx, &mut sidebar, 2, before[0] - 3.0);
    settle(&ctx, &mut sidebar);
    let after = label_ys(&ctx, &mut sidebar, ["fa", "fb", "fc"]);
    assert!((after[0] - (before[0] + PITCH)).abs() < 0.01, "{after:?}");
    assert!((after[1] - (before[1] + PITCH)).abs() < 0.01, "{after:?}");
}

#[test]
fn the_lifted_row_follows_the_pointer() {
    let (ctx, mut sidebar) = (ready(), three());
    warm_up(&ctx, &mut sidebar);
    let before = label_ys(&ctx, &mut sidebar, ["fa", "fb", "fc"]);
    let texts = frame(&ctx, &mut sidebar, Vec::new());
    let grip_y = texts.iter().find(|t| t.string == "::").unwrap().pos.y + 5.0;
    drag_to(&ctx, &mut sidebar, 0, grip_y + 20.0);
    let after = label_ys(&ctx, &mut sidebar, ["fa", "fb", "fc"]);
    assert!((after[0] - (before[0] + 20.0)).abs() < 0.01, "{after:?}");
}

#[test]
fn the_lifted_row_stays_inside_the_list() {
    let (ctx, mut sidebar) = (ready(), three());
    warm_up(&ctx, &mut sidebar);
    let before = label_ys(&ctx, &mut sidebar, ["fa", "fb", "fc"]);
    drag_to(&ctx, &mut sidebar, 0, 290.0);
    settle(&ctx, &mut sidebar);
    let after = label_ys(&ctx, &mut sidebar, ["fa", "fb", "fc"]);
    assert!((after[0] - before[2]).abs() < 0.01, "{after:?}");
    assert!(
        (after[1] - before[1]).abs() < 0.01,
        "outside the list nothing slides: {after:?}"
    );
}

#[test]
fn escape_slides_the_rows_back() {
    let (ctx, mut sidebar) = (ready(), three());
    warm_up(&ctx, &mut sidebar);
    let before = label_ys(&ctx, &mut sidebar, ["fa", "fb", "fc"]);
    drag_to(&ctx, &mut sidebar, 2, before[0] - 3.0);
    settle(&ctx, &mut sidebar);
    let escape = Event::Key {
        key: Key::Escape,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::NONE,
    };
    frame(&ctx, &mut sidebar, vec![escape]);
    settle(&ctx, &mut sidebar);
    assert_eq!(label_ys(&ctx, &mut sidebar, ["fa", "fb", "fc"]), before);
}

#[test]
fn a_dropped_row_settles_in_its_new_place_after_set_favorites() {
    let (ctx, mut sidebar) = (ready(), three());
    warm_up(&ctx, &mut sidebar);
    let before = label_ys(&ctx, &mut sidebar, ["fa", "fb", "fc"]);
    let end = drag_to(&ctx, &mut sidebar, 2, before[0] - 3.0);
    settle(&ctx, &mut sidebar);
    release(&ctx, &mut sidebar, end);
    let Some(SidebarEvent::Reorder(paths)) = sidebar.take_events().pop() else {
        panic!("no reorder");
    };
    let mut rows = sidebar.state.favorites().to_vec();
    rows.sort_by_key(|row| paths.iter().position(|p| *p == row.path));
    sidebar.set_favorites(rows);
    let moved = label_ys(&ctx, &mut sidebar, ["fc", "fa", "fb"]);
    assert!(
        (moved[1] - before[1]).abs() < 0.01,
        "no jump for fa: {moved:?}"
    );
    settle(&ctx, &mut sidebar);
    assert_eq!(label_ys(&ctx, &mut sidebar, ["fc", "fa", "fb"]), before);
}

/// The slot for a pointer at `y` over three 27 point rows from y = 100, with the whole screen as clip.
fn slot_at(y: f32) -> Option<usize> {
    slot_in_clip(y, Rect::from_min_size(Pos2::ZERO, vec2(400.0, 300.0)))
}

/// The slot for a pointer at `y` over the same rows, with `clip` as the clip rect.
fn slot_in_clip(y: f32, clip: Rect) -> Option<usize> {
    let ctx = ready();
    let spans = [(100.0, 127.0), (127.0, 154.0), (154.0, 181.0)];
    let mut slot = None;
    for events in [vec![Event::PointerMoved(pos2(50.0, y))], Vec::new()] {
        let raw = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(400.0, 300.0))),
            events,
            ..Default::default()
        };
        let output = ctx.run_ui(raw, |ui| {
            ui.set_clip_rect(clip);
            slot = super::favorites::pointer_slot(ui, &spans);
        });
        output.drop_without_applying_deltas();
    }
    slot
}

#[test]
fn a_pointer_above_the_list_inside_the_clip_means_the_first_slot() {
    assert_eq!(slot_at(40.0), Some(0));
}

#[test]
fn a_pointer_below_the_list_inside_the_clip_means_the_last_slot() {
    assert_eq!(slot_at(250.0), Some(3));
}

#[test]
fn a_pointer_on_a_row_keeps_its_slot() {
    assert_eq!(slot_at(130.0), Some(1));
}

#[test]
fn a_pointer_outside_the_clip_has_no_slot() {
    let clip = Rect::from_min_max(pos2(0.0, 90.0), pos2(400.0, 190.0));
    assert_eq!(slot_in_clip(250.0, clip), None);
    // The control: inside the narrowed clip, below the list still gives a slot.
    assert_eq!(slot_in_clip(185.0, clip), Some(3));
}

/// The y of each label in `names` below the FAVORITES heading, in one frame.
///
/// The favorites panel sits at the bottom and shrinks with the list, so
/// heights are measured from its heading.
fn below_head(ctx: &Context, sidebar: &mut Sidebar, names: &[&str]) -> Vec<f32> {
    let texts = frame(ctx, sidebar, Vec::new());
    let head = find(&texts, "FAVORITES").pos.y;
    names
        .iter()
        .map(|name| find(&texts, name).pos.y - head)
        .collect()
}

#[test]
fn the_rows_below_a_removed_favorite_slide_up() {
    let (ctx, mut sidebar) = (ready(), three());
    warm_up(&ctx, &mut sidebar);
    let before = below_head(&ctx, &mut sidebar, &["fa", "fb", "fc"]);
    let rest = sidebar.state.favorites()[1..].to_vec();
    sidebar.set_favorites(rest);
    let first = below_head(&ctx, &mut sidebar, &["fb"])[0];
    assert!(
        first > before[0] + 0.01,
        "fb jumped to its new place: {first}"
    );
    assert!(
        first <= before[1] + 0.01,
        "fb starts below its old place: {first}"
    );
    settle(&ctx, &mut sidebar);
    let after = below_head(&ctx, &mut sidebar, &["fb", "fc"]);
    assert!((after[0] - before[0]).abs() < 0.01, "{after:?} {before:?}");
    assert!((after[1] - before[1]).abs() < 0.01, "{after:?} {before:?}");
}
