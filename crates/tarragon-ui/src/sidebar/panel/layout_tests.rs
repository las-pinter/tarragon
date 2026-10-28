//! Tests for the panel layout: the favorites stay in view under a tall tree.

use super::tests::{Painted, find, frame, frame_sized, ready};
use super::*;
use egui::Context;
use egui::{Event, Modifiers, MouseWheelUnit, PointerButton, Pos2};

/// A panel with root `/r` over `folders` children and `count` favorites named `f0`, `f1`, ...
fn crowded(folders: usize, count: usize) -> Sidebar {
    let mut sidebar = Sidebar::default();
    sidebar.set_roots(vec![PathBuf::from("/r")]);
    let kids = (0..folders)
        .map(|n| PathBuf::from(format!("/r/d{n}")))
        .collect();
    sidebar.tree_mut().set_children(Path::new("/r"), kids);
    sidebar.tree_mut().set_expanded(Path::new("/r"), true);
    sidebar.set_favorites(
        (0..count)
            .map(|n| FavoriteRow {
                path: PathBuf::from(format!("/f/{n}")),
                label: Some(format!("f{n}")),
                sort_order: i64::try_from(n).unwrap(),
                missing: false,
            })
            .collect(),
    );
    sidebar.take_events();
    sidebar
}

/// The favorite rows for `count` favorites.
fn favorite_rows(count: usize) -> Vec<FavoriteRow> {
    (0..count)
        .map(|n| FavoriteRow {
            path: PathBuf::from(format!("/f/{n}")),
            label: Some(format!("f{n}")),
            sort_order: i64::try_from(n).unwrap(),
            missing: false,
        })
        .collect()
}

/// Run `count` frames without events and return the texts of the last one.
fn settle(ctx: &Context, sidebar: &mut Sidebar, count: usize) -> Vec<Painted> {
    let mut texts = Vec::new();
    for _ in 0..count {
        texts = frame(ctx, sidebar, Vec::new());
    }
    texts
}

/// Whether the last frame's repaint request came from `panel.rs`.
///
/// egui keeps the causes of a frame until the next frame starts, so call this
/// after the frame that follows the one under test. The scroll areas ask for
/// repaints on their own in these tests, so the cause is matched by file.
fn panel_asked_for_a_repaint(ctx: &Context) -> bool {
    ctx.repaint_causes()
        .iter()
        .any(|cause| Path::new(cause.file).ends_with("sidebar/panel.rs"))
}

/// A pointer button event.
fn press(pressed: bool, pos: Pos2) -> Event {
    Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    }
}

/// Press at `start`, drag to `end`, and stop before the release.
fn drag(ctx: &Context, sidebar: &mut Sidebar, start: Pos2, end: Pos2) {
    frame(ctx, sidebar, vec![Event::PointerMoved(start)]);
    frame(ctx, sidebar, vec![press(true, start)]);
    frame(
        ctx,
        sidebar,
        vec![Event::PointerMoved(start + vec2(0.0, 8.0))],
    );
    frame(ctx, sidebar, vec![Event::PointerMoved(end)]);
    frame(ctx, sidebar, Vec::new());
}

/// The grips of the rows inside the favorites viewport, with their row index.
fn visible_grips(texts: &[Painted]) -> Vec<(usize, Pos2)> {
    let top = find(texts, "FAVORITES").pos.y + 16.0;
    let bottom = find(texts, "Add Current Folder").pos.y - 8.0;
    texts
        .iter()
        .filter(|t| t.string == "::")
        .enumerate()
        .filter(|(_, t)| t.pos.y >= top && t.pos.y + 14.0 <= bottom)
        .map(|(index, t)| (index, t.pos))
        .collect()
}

#[test]
fn the_favorites_stay_on_screen_under_a_tall_tree() {
    let mut sidebar = crowded(60, 3);
    let texts = settle(&ready(), &mut sidebar, 3);
    let buttons = find(&texts, "Add Current Folder").pos.y;
    let head = find(&texts, "FAVORITES").pos.y;
    let first = find(&texts, "f0").pos.y;
    assert!(head >= 0.0 && head + 14.0 <= first, "head at {head}");
    assert!(
        first + 14.0 <= buttons,
        "f0 at {first}, buttons at {buttons}"
    );
    assert!(buttons + 14.0 <= 300.0);
}

/// The height of the favorites panel in a settled frame.
///
/// It runs from 22 px above the head text (gap 14, margin 8) to 11 px above
/// the button text (margin 8, button padding 3).
fn panel_height(texts: &[Painted]) -> f32 {
    (find(texts, "Add Current Folder").pos.y - 11.0) - (find(texts, "FAVORITES").pos.y - 22.0)
}

/// The height of the favorites head: gap 14, margins 8 and 4, and the text.
fn head_height(texts: &[Painted]) -> f32 {
    26.0 + find(texts, "FAVORITES").height
}

#[test]
fn forty_favorites_take_half_the_dock_and_the_tree_shows() {
    let mut sidebar = crowded(60, 40);
    let texts = settle(&ready(), &mut sidebar, 4);
    let head = find(&texts, "FAVORITES").pos.y;
    let buttons = find(&texts, "Add Current Folder").pos.y;
    assert!(sidebar.favorites_height > 150.0, "the rows must overflow");
    let panel = panel_height(&texts);
    assert!(
        (panel - 150.0).abs() <= 1.0,
        "favorites panel is {panel} high"
    );
    assert!(find(&texts, "f0").pos.y > head && find(&texts, "f0").pos.y < buttons);
    let tree_rows = texts
        .iter()
        .filter(|t| t.string.starts_with('d') && t.pos.y >= 0.0 && t.pos.y + 14.0 <= head)
        .count();
    assert!(tree_rows > 0, "no tree rows above the favorites");
}

#[test]
fn three_favorites_take_the_head_and_the_rows() {
    let mut sidebar = crowded(60, 3);
    let texts = settle(&ready(), &mut sidebar, 4);
    let content = sidebar.favorites_height;
    assert!(content > 64.0 && content < 150.0, "content is {content}");
    let panel = panel_height(&texts);
    let want = head_height(&texts) + content;
    assert!((panel - want).abs() <= 1.0, "panel {panel}, want {want}");
}

#[test]
fn two_favorites_are_not_forced_taller_than_their_rows() {
    let mut sidebar = crowded(60, 2);
    let texts = settle(&ready(), &mut sidebar, 4);
    let content = sidebar.favorites_height;
    assert!(content < 64.0, "content is {content}");
    let panel = panel_height(&texts);
    let want = head_height(&texts) + content;
    assert!((panel - want).abs() <= 1.0, "panel {panel}, want {want}");
}

/// The height of one favorite row, measured from `count` rows in the panel.
fn row_height(sidebar: &Sidebar, count: u8) -> f32 {
    sidebar.favorites_height / f32::from(count)
}

#[test]
fn a_changed_favorites_height_asks_for_a_repaint_and_settles() {
    let ctx = ready();
    let mut sidebar = crowded(60, 3);
    settle(&ctx, &mut sidebar, 8);
    assert!(!panel_asked_for_a_repaint(&ctx), "repaint at rest");
    let row = row_height(&sidebar, 3);
    for count in [4_u8, 3] {
        sidebar.set_favorites(favorite_rows(usize::from(count)));
        frame(&ctx, &mut sidebar, Vec::new());
        frame(&ctx, &mut sidebar, Vec::new());
        assert!(panel_asked_for_a_repaint(&ctx), "no repaint at {count}");
        // The repaint frame feeds the new minimum height; the panel follows
        // one frame later, because egui sizes it from the frame before.
        let got = frame(&ctx, &mut sidebar, Vec::new());
        // A growing panel changes its rect in the repaint frame, and the
        // panel asks for frame 3 itself. A shrinking one is done by then.
        if count == 4 {
            assert!(panel_asked_for_a_repaint(&ctx), "no frame 3 at {count}");
        }
        let panel = panel_height(&got);
        let want = head_height(&got) + row * f32::from(count);
        assert!(
            (panel - want).abs() <= 1.0,
            "{count} favorites: panel {panel}, want {want}"
        );
    }
}

#[test]
fn a_dock_that_grows_over_a_scrolling_list_reaches_the_new_cap_on_its_own() {
    let ctx = ready();
    let mut sidebar = crowded(60, 40);
    settle(&ctx, &mut sidebar, 6);
    assert!((panel_height(&settle(&ctx, &mut sidebar, 1)) - 150.0).abs() <= 1.0);
    let big = vec2(400.0, 500.0);
    // Run frames only while the last frame's repaint requests came from the panel.
    let mut texts = frame_sized(&ctx, &mut sidebar, Vec::new(), big);
    for _ in 0..10 {
        let next = frame_sized(&ctx, &mut sidebar, Vec::new(), big);
        if !panel_asked_for_a_repaint(&ctx) {
            break;
        }
        texts = next;
    }
    let panel = panel_height(&texts);
    assert!(
        (panel - 250.0).abs() <= 1.0,
        "favorites panel is {panel} high"
    );
}

#[test]
fn a_drag_reorder_inside_the_scrolled_favorites_area_gives_the_new_order() {
    let mut sidebar = crowded(60, 40);
    let ctx = ready();
    let texts = settle(&ctx, &mut sidebar, 3);
    let over = find(&texts, "f0").pos + vec2(20.0, 5.0);
    frame(&ctx, &mut sidebar, vec![Event::PointerMoved(over)]);
    let wheel = Event::MouseWheel {
        unit: MouseWheelUnit::Point,
        delta: vec2(0.0, -200.0),
        phase: egui::TouchPhase::Move,
        modifiers: Modifiers::NONE,
    };
    frame(&ctx, &mut sidebar, vec![wheel]);
    let texts = settle(&ctx, &mut sidebar, 30);
    let grips = visible_grips(&texts);
    let ((a, _), (b, grip)) = (grips[0], grips[1]);
    assert!(a > 0, "the list did not scroll");
    let top = find(&texts, &format!("f{a}")).pos.y;
    let start = grip + vec2(2.0, 5.0);
    drag(&ctx, &mut sidebar, start, Pos2::new(start.x, top - 3.0));
    frame(
        &ctx,
        &mut sidebar,
        vec![press(false, Pos2::new(start.x, top - 3.0))],
    );
    let mut want: Vec<PathBuf> = favorite_rows(40).into_iter().map(|f| f.path).collect();
    let moved = want.remove(b);
    want.insert(a, moved);
    assert_eq!(sidebar.take_events(), [SidebarEvent::Reorder(want)]);
}

#[test]
fn a_drag_released_below_the_favorites_viewport_queues_nothing() {
    let mut sidebar = crowded(60, 40);
    let ctx = ready();
    let texts = settle(&ctx, &mut sidebar, 3);
    let grip = texts.iter().find(|t| t.string == "::").unwrap().pos;
    let start = grip + vec2(2.0, 5.0);
    let end = Pos2::new(start.x, find(&texts, "Add Current Folder").pos.y);
    drag(&ctx, &mut sidebar, start, end);
    frame(&ctx, &mut sidebar, vec![press(false, end)]);
    assert_eq!(sidebar.take_events(), Vec::<SidebarEvent>::new());
    assert_eq!(sidebar.dragging, None);
}
