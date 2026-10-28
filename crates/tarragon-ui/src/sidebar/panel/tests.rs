//! Tests for the Library panel.

use super::*;
use crate::sidebar::paint::ICON_WIDTH;
use crate::sidebar::row::{CARET_SLOT, GAP};
use egui::{Color32, Context, Event, Modifiers, PointerButton, Pos2, RawInput, Rect, Shape};

/// One painted text.
pub(super) struct Painted {
    /// The string.
    pub(super) string: String,
    /// The top-left corner.
    pub(super) pos: Pos2,
    /// The height of the laid-out text.
    pub(super) height: f32,
    /// The colour of its first section.
    color: Color32,
}

/// A favorite row.
fn favorite(missing: bool) -> FavoriteRow {
    FavoriteRow {
        path: PathBuf::from("/f/x"),
        label: Some("fav".to_owned()),
        sort_order: 0,
        missing,
    }
}

/// A panel with root `/r` open over `count` children named `a`, `b`, ..., and one favorite.
fn panel_with(count: usize) -> Sidebar {
    let mut sidebar = Sidebar::default();
    sidebar.set_roots(vec![PathBuf::from("/r")]);
    let kids = (0..count)
        .map(|n| {
            PathBuf::from(format!(
                "/r/{}",
                (b'a' + u8::try_from(n % 26).unwrap()) as char
            ))
        })
        .collect();
    sidebar.tree_mut().set_children(Path::new("/r"), kids);
    sidebar.tree_mut().set_expanded(Path::new("/r"), true);
    sidebar.set_favorites(vec![favorite(false)]);
    sidebar.take_events();
    sidebar
}

/// A panel with two tree children.
fn panel() -> Sidebar {
    panel_with(2)
}

/// A context with the theme.
pub(super) fn ready() -> Context {
    let ctx = Context::default();
    crate::apply_theme(&ctx);
    ctx
}

/// Run one frame on a 400 by 300 screen and return the painted texts.
pub(super) fn frame(ctx: &Context, sidebar: &mut Sidebar, events: Vec<Event>) -> Vec<Painted> {
    frame_sized(ctx, sidebar, events, vec2(400.0, 300.0))
}

/// Run one frame on a screen of `size` and return the painted texts.
pub(super) fn frame_sized(
    ctx: &Context,
    sidebar: &mut Sidebar,
    events: Vec<Event>,
    size: egui::Vec2,
) -> Vec<Painted> {
    let raw = RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
        events,
        ..Default::default()
    };
    let output = ctx.run_ui(raw, |ui| sidebar.show(ui));
    let texts = output
        .shapes
        .iter()
        .filter_map(|clipped| match &clipped.shape {
            Shape::Text(text) => Some(Painted {
                string: text.galley.job.text.clone(),
                pos: text.pos,
                height: text.galley.size().y,
                color: text.galley.job.sections[0].format.color,
            }),
            _ => None,
        })
        .collect();
    output.drop_without_applying_deltas();
    texts
}

/// The first painted text equal to `wanted`.
pub(super) fn find<'a>(texts: &'a [Painted], wanted: &str) -> &'a Painted {
    texts
        .iter()
        .find(|text| text.string == wanted)
        .unwrap_or_else(|| panic!("no text {wanted}"))
}

/// Run frames until the layout settles.
///
/// egui sizes the favorites panel from the frame before, so a new list needs
/// a few frames before the rows stand where a test looks for them. Three
/// frames are the most a new list needs: measure, apply the minimum, resize.
pub(super) fn warm_up(ctx: &Context, sidebar: &mut Sidebar) {
    for _ in 0..3 {
        frame(ctx, sidebar, Vec::new());
    }
}

/// Click at `offset` from the text `wanted`, after the layout settles, over three frames.
pub(super) fn click(ctx: &Context, sidebar: &mut Sidebar, wanted: &str, offset: egui::Vec2) {
    warm_up(ctx, sidebar);
    let pos = find(&frame(ctx, sidebar, Vec::new()), wanted).pos + offset;
    let press = |pressed| Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    };
    frame(ctx, sidebar, vec![Event::PointerMoved(pos)]);
    frame(ctx, sidebar, vec![press(true)]);
    frame(ctx, sidebar, vec![press(false)]);
}

/// The events left after clicking the text `wanted`.
fn events_after(sidebar: &mut Sidebar, wanted: &str) -> Vec<SidebarEvent> {
    click(&ready(), sidebar, wanted, vec2(2.0, 5.0));
    sidebar.take_events()
}

#[test]
fn a_tree_row_click_navigates_once() {
    let mut sidebar = panel();
    let events = events_after(&mut sidebar, "a");
    assert_eq!(events, [SidebarEvent::Navigate(PathBuf::from("/r/a"))]);
    assert_eq!(sidebar.open_folder(), Some(Path::new("/r/a")));
}

#[test]
fn salt_0_rows_in_both_lists_act_alone() {
    let mut sidebar = panel();
    let events = events_after(&mut sidebar, "r");
    assert_eq!(events, [SidebarEvent::Navigate(PathBuf::from("/r"))]);
    let events = events_after(&mut sidebar, "fav");
    assert_eq!(events, [SidebarEvent::Navigate(PathBuf::from("/f/x"))]);
}

#[test]
fn a_caret_click_toggles_and_queues_nothing() {
    let mut sidebar = panel();
    let to_caret = -(GAP + ICON_WIDTH + GAP + CARET_SLOT / 2.0);
    click(&ready(), &mut sidebar, "r", vec2(to_caret, 5.0));
    assert_eq!(sidebar.take_events(), Vec::<SidebarEvent>::new());
    assert_eq!(sidebar.tree_mut().rows().len(), 1);
}

#[test]
fn a_favorite_click_selects_it_and_opens_it() {
    let mut sidebar = panel();
    let events = events_after(&mut sidebar, "fav");
    assert_eq!(events, [SidebarEvent::Navigate(PathBuf::from("/f/x"))]);
    assert_eq!(sidebar.selected_favorite(), Some(Path::new("/f/x")));
    assert_eq!(sidebar.open_folder(), Some(Path::new("/f/x")));
}

#[test]
fn remove_needs_a_selected_favorite_and_clears_it() {
    let mut sidebar = panel();
    let ctx = ready();
    click(&ctx, &mut sidebar, "Remove", vec2(2.0, 3.0));
    assert_eq!(sidebar.take_events(), Vec::<SidebarEvent>::new());
    click(&ctx, &mut sidebar, "fav", vec2(2.0, 5.0));
    sidebar.take_events();
    click(&ctx, &mut sidebar, "Remove", vec2(2.0, 3.0));
    assert_eq!(
        sidebar.take_events(),
        [SidebarEvent::Remove(PathBuf::from("/f/x"))]
    );
    assert_eq!(sidebar.selected_favorite(), Some(Path::new("/f/x")));
    sidebar.set_favorites(Vec::new());
    assert_eq!(sidebar.selected_favorite(), None);
}

#[test]
fn clicking_a_tree_row_that_is_not_a_favorite_clears_the_selection() {
    let mut sidebar = panel();
    events_after(&mut sidebar, "fav");
    assert_eq!(sidebar.selected_favorite(), Some(Path::new("/f/x")));
    events_after(&mut sidebar, "a");
    assert_eq!(sidebar.open_folder(), Some(Path::new("/r/a")));
    assert_eq!(sidebar.selected_favorite(), None);
}

#[test]
fn clicking_a_tree_row_equal_to_a_favorite_selects_that_favorite() {
    let mut sidebar = panel();
    let mut row = favorite(false);
    row.path = PathBuf::from("/r/a");
    sidebar.set_favorites(vec![row]);
    events_after(&mut sidebar, "a");
    assert_eq!(sidebar.selected_favorite(), Some(Path::new("/r/a")));
}

#[test]
fn set_favorites_drops_a_selection_whose_row_is_gone() {
    let mut sidebar = panel();
    events_after(&mut sidebar, "fav");
    sidebar.set_favorites(vec![favorite(true)]);
    assert_eq!(sidebar.selected_favorite(), Some(Path::new("/f/x")));
    sidebar.set_favorites(Vec::new());
    assert_eq!(sidebar.selected_favorite(), None);
}

#[test]
fn add_queues_add_current_and_take_drains() {
    let mut sidebar = panel();
    click(&ready(), &mut sidebar, "Add Current Folder", vec2(2.0, 3.0));
    assert_eq!(sidebar.take_events(), [SidebarEvent::AddCurrent]);
    assert_eq!(sidebar.take_events(), Vec::<SidebarEvent>::new());
}

#[test]
fn the_heads_and_the_empty_text_are_painted() {
    let mut sidebar = Sidebar::default();
    let ctx = ready();
    warm_up(&ctx, &mut sidebar);
    let texts = frame(&ctx, &mut sidebar, Vec::new());
    for wanted in ["FOLDERS", "FAVORITES", "No favorites yet."] {
        find(&texts, wanted);
    }
}

#[test]
fn a_missing_favorite_paints_dim() {
    let mut sidebar = panel();
    let ctx = ready();
    let found = find(&frame(&ctx, &mut sidebar, Vec::new()), "fav").color;
    sidebar.set_favorites(vec![favorite(true)]);
    let gone = find(&frame(&ctx, &mut sidebar, Vec::new()), "fav").color;
    assert_eq!((found, gone), (tokens::TEXT_SECONDARY, tokens::TEXT_MUTED));
}

#[test]
fn the_buttons_stay_on_screen_with_a_tall_tree() {
    let mut sidebar = panel_with(60);
    let texts = frame(&ready(), &mut sidebar, Vec::new());
    for wanted in ["Add Current Folder", "Remove"] {
        let y = find(&texts, wanted).pos.y;
        assert!(y > 0.0 && y < 300.0, "{wanted} at {y}");
    }
}

/// A panel with root `/r` and nothing listed; the root events are taken.
fn unlisted() -> Sidebar {
    let mut sidebar = Sidebar::default();
    sidebar.set_roots(vec![PathBuf::from("/r")]);
    sidebar.take_events();
    sidebar
}

fn list(path: &str) -> SidebarEvent {
    SidebarEvent::ListChildren(PathBuf::from(path))
}

#[test]
fn set_roots_queues_one_listing_per_root() {
    let mut sidebar = Sidebar::default();
    sidebar.set_roots(vec![PathBuf::from("/r"), PathBuf::from("/s")]);
    assert_eq!(sidebar.take_events(), [list("/r"), list("/s")]);
}

#[test]
fn expanding_an_unlisted_node_asks_once_even_after_two_clicks() {
    let mut sidebar = unlisted();
    sidebar.set_children(Path::new("/r"), vec![PathBuf::from("/r/a")]);
    sidebar.tree_mut().set_expanded(Path::new("/r"), true);
    let ctx = ready();
    let to_caret = -(GAP + ICON_WIDTH + GAP + CARET_SLOT / 2.0);
    click(&ctx, &mut sidebar, "a", vec2(to_caret, 5.0));
    assert_eq!(sidebar.take_events(), [list("/r/a")]);
    click(&ctx, &mut sidebar, "a", vec2(to_caret, 5.0));
    click(&ctx, &mut sidebar, "a", vec2(to_caret, 5.0));
    frame(&ctx, &mut sidebar, Vec::new());
    assert_eq!(sidebar.take_events(), Vec::<SidebarEvent>::new());
}

#[test]
fn set_children_answers_the_request_and_shows_the_rows() {
    let mut sidebar = unlisted();
    sidebar.tree_mut().set_expanded(Path::new("/r"), true);
    sidebar.set_children(Path::new("/r"), vec![PathBuf::from("/r/a")]);
    let texts = frame(&ready(), &mut sidebar, Vec::new());
    find(&texts, "a");
    assert!(sidebar.pending.is_empty());
}

#[test]
fn set_open_folder_opens_ancestors_and_asks_for_the_unlisted() {
    let mut sidebar = Sidebar::default();
    sidebar.set_roots(vec![PathBuf::from("/r")]);
    sidebar.set_open_folder(Some(PathBuf::from("/r/a/b")));
    assert_eq!(sidebar.take_events(), [list("/r")]);
    sidebar.set_children(Path::new("/r"), vec![PathBuf::from("/r/a")]);
    assert_eq!(sidebar.take_events(), [list("/r/a")]);
    sidebar.set_children(Path::new("/r/a"), vec![PathBuf::from("/r/a/b")]);
    assert_eq!(sidebar.take_events(), Vec::<SidebarEvent>::new());
    let texts = frame(&ready(), &mut sidebar, Vec::new());
    find(&texts, "b");
}

#[test]
fn an_open_folder_outside_all_roots_expands_nothing() {
    let mut sidebar = unlisted();
    sidebar.set_open_folder(Some(PathBuf::from("/elsewhere/x")));
    assert_eq!(sidebar.take_events(), Vec::<SidebarEvent>::new());
    assert_eq!(
        sidebar
            .tree_mut()
            .rows()
            .iter()
            .filter(|r| r.expanded)
            .count(),
        0
    );
}

#[test]
fn an_unknown_ancestor_ends_the_walk_without_a_loop() {
    let mut sidebar = unlisted();
    sidebar.set_open_folder(Some(PathBuf::from("/r/.h/x")));
    assert_eq!(sidebar.take_events(), Vec::<SidebarEvent>::new());
    sidebar.set_children(Path::new("/r"), vec![PathBuf::from("/r/a")]);
    assert_eq!(sidebar.take_events(), Vec::<SidebarEvent>::new());
    assert!(sidebar.pending.is_empty());
}

#[test]
fn an_unrelated_answer_keeps_a_collapsed_ancestor_closed() {
    let mut sidebar = unlisted();
    sidebar.set_children(
        Path::new("/r"),
        vec![PathBuf::from("/r/a"), PathBuf::from("/r/c")],
    );
    sidebar.set_open_folder(Some(PathBuf::from("/r/a/b")));
    sidebar.take_events();
    sidebar.tree_mut().set_expanded(Path::new("/r"), false);
    sidebar.pending.insert(PathBuf::from("/r/c"));
    sidebar.set_children(Path::new("/r/c"), vec![PathBuf::from("/r/c/d")]);
    assert!(!sidebar.tree_mut().rows()[0].expanded);
}

#[test]
fn a_removed_root_forgets_its_request_in_flight() {
    let mut sidebar = unlisted();
    sidebar.set_roots(Vec::new());
    sidebar.set_roots(vec![PathBuf::from("/r")]);
    assert_eq!(sidebar.take_events(), [list("/r")]);
}

#[test]
fn nested_roots_duplicate_roots_and_a_root_as_open_folder() {
    let mut sidebar = Sidebar::default();
    sidebar.set_roots(vec![PathBuf::from("/h"), PathBuf::from("/h/cli")]);
    sidebar.set_open_folder(Some(PathBuf::from("/h/cli/x")));
    assert_eq!(sidebar.take_events(), [list("/h"), list("/h/cli")]);
    let mut sidebar = Sidebar::default();
    sidebar.set_roots(vec![PathBuf::from("/r"), PathBuf::from("/r")]);
    assert_eq!(sidebar.take_events(), [list("/r")]);
    let mut sidebar = unlisted();
    sidebar.set_open_folder(Some(PathBuf::from("/r")));
    assert_eq!(sidebar.take_events(), Vec::<SidebarEvent>::new());
}

#[test]
fn the_open_folder_selects_the_favorite_with_that_path() {
    let mut sidebar = Sidebar::default();
    sidebar.set_favorites(vec![favorite(false)]);
    sidebar.set_open_folder(Some(PathBuf::from("/f/other")));
    assert_eq!(sidebar.selected_favorite(), None);
    sidebar.set_open_folder(Some(PathBuf::from("/f/x")));
    assert_eq!(sidebar.selected_favorite(), Some(Path::new("/f/x")));
    sidebar.set_open_folder(Some(PathBuf::from("/f/other")));
    assert_eq!(
        sidebar.selected_favorite(),
        None,
        "another folder clears it"
    );
    sidebar.set_open_folder(Some(PathBuf::from("/f/x")));
    sidebar.set_open_folder(None);
    assert_eq!(sidebar.selected_favorite(), None);
}

#[test]
fn a_new_favorite_list_selects_the_open_folder_when_it_matches() {
    let mut sidebar = Sidebar::default();
    sidebar.set_open_folder(Some(PathBuf::from("/f/x")));
    assert_eq!(sidebar.selected_favorite(), None);
    sidebar.set_favorites(vec![favorite(false)]);
    assert_eq!(sidebar.selected_favorite(), Some(Path::new("/f/x")));
}
