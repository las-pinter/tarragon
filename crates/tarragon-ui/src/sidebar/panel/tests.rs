//! Tests for the Library panel.

use super::*;
use crate::sidebar::paint::ICON_WIDTH;
use crate::sidebar::row::{CARET_SLOT, GAP};
use egui::{Color32, Context, Event, Modifiers, PointerButton, Pos2, RawInput, Rect, Shape};

/// One painted text.
struct Painted {
    /// The string.
    string: String,
    /// The top-left corner.
    pos: Pos2,
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
    sidebar
}

/// A panel with two tree children.
fn panel() -> Sidebar {
    panel_with(2)
}

/// A context with the theme.
fn ready() -> Context {
    let ctx = Context::default();
    crate::apply_theme(&ctx);
    ctx
}

/// Run one frame on a 400 by 300 screen and return the painted texts.
fn frame(ctx: &Context, sidebar: &mut Sidebar, events: Vec<Event>) -> Vec<Painted> {
    let raw = RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(400.0, 300.0))),
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
                color: text.galley.job.sections[0].format.color,
            }),
            _ => None,
        })
        .collect();
    output.drop_without_applying_deltas();
    texts
}

/// The first painted text equal to `wanted`.
fn find<'a>(texts: &'a [Painted], wanted: &str) -> &'a Painted {
    texts
        .iter()
        .find(|text| text.string == wanted)
        .unwrap_or_else(|| panic!("no text {wanted}"))
}

/// Click at `offset` from the text `wanted`, over three frames.
fn click(ctx: &Context, sidebar: &mut Sidebar, wanted: &str, offset: egui::Vec2) {
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
    assert_eq!(sidebar.selected_favorite(), None);
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
    let texts = frame(&ready(), &mut sidebar, Vec::new());
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
