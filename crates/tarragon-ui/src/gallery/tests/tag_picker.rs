//! Tests for the tag picker.

use super::context_menu::{key, menu_shows};
use super::rename_dialog::{click_button, has_text, texts};
use super::*;
use crate::gallery::tag_picker::MENU_ID;
use crate::grid::CellClick;
use crate::test_support;
use egui::{Context, Key};

/// The `/images` paths of `names`.
fn paths(names: &[&str]) -> Vec<PathBuf> {
    names
        .iter()
        .map(|name| PathBuf::from("/images").join(name))
        .collect()
}

/// Draw `count` empty frames.
fn settle(ctx: &Context, gallery: &mut Gallery, count: usize) {
    for _ in 0..count {
        texts(ctx, gallery, Vec::new());
    }
}

/// A gallery over `/images` that knows the tags "fox" and "owl", drawn.
fn start() -> (Context, Gallery) {
    let ctx = test_support::context();
    crate::theme::apply_theme(&ctx);
    let mut gallery = Gallery::new(SortMode::Natural);
    gallery.set_folder(Some(PathBuf::from("/images")));
    gallery.set_items(items());
    gallery.set_filter_lists(vec!["fox".to_owned(), "owl".to_owned()], Vec::new());
    settle(&ctx, &mut gallery, 3);
    (ctx, gallery)
}

/// Like [`start`], with the picker open at `at` on a.png and b.png, settled.
fn open(at: Pos2) -> (Context, Gallery) {
    let (ctx, mut gallery) = start();
    gallery.open_tag_picker(at, paths(&["a.png", "b.png"]));
    settle(&ctx, &mut gallery, 3);
    (ctx, gallery)
}

/// Like [`open`], with the name input showing and focused.
fn open_input() -> (Context, Gallery) {
    let (ctx, mut gallery) = open(pos2(300.0, 300.0));
    click_button(&ctx, &mut gallery, "Create new...");
    settle(&ctx, &mut gallery, 3);
    (ctx, gallery)
}

/// The rect of the menu area.
fn menu_rect(ctx: &Context) -> Rect {
    ctx.memory(|memory| memory.area_rect(Id::new(MENU_ID)))
        .expect("the menu was drawn")
}

/// Press Enter in the input, then draw frames so the answer shows.
fn enter(ctx: &Context, gallery: &mut Gallery) {
    texts(ctx, gallery, vec![key(Key::Enter)]);
    settle(ctx, gallery, 2);
}

#[test]
fn the_menu_opens_with_its_top_left_at_the_pointer() {
    let at = pos2(300.0, 200.0);
    let (ctx, _gallery) = open(at);
    let rect = menu_rect(&ctx);
    assert_eq!(rect.min.x, at.x, "the left edge is at the pointer");
    // The 1 by 1 anchor and the gap lie between the pointer and the menu.
    assert_eq!(rect.min.y, at.y + 1.0 + tokens::SPACING_XS);
}

#[test]
fn the_menu_moves_inside_the_window_near_an_edge() {
    let bounds = screen().shrink(tokens::MENU_EDGE_MARGIN);
    for at in [pos2(1199.0, 799.0), pos2(0.0, 799.0), pos2(1199.0, 0.0)] {
        let (ctx, _gallery) = open(at);
        let rect = menu_rect(&ctx);
        assert!(
            bounds.expand(0.5).contains_rect(rect),
            "{rect:?} is outside {bounds:?} for {at:?}"
        );
        assert!(rect.width() >= 90.0, "{rect:?} is squeezed for {at:?}");
    }
}

#[test]
fn picking_a_known_tag_gives_every_path_and_the_tag() {
    let (ctx, mut gallery) = open(pos2(300.0, 300.0));
    let seen = texts(&ctx, &mut gallery, Vec::new());
    assert!(has_text(&seen, "fox") && has_text(&seen, "owl"));

    click_button(&ctx, &mut gallery, "owl");
    assert_eq!(
        gallery.take_menu_events(),
        vec![GridMenuEvent::TagPaths {
            paths: paths(&["a.png", "b.png"]),
            tag: "owl".to_owned(),
        }]
    );
    assert!(!gallery.tag_picker_open(), "the picker closed");
}

#[test]
fn a_refused_new_name_shows_the_reason_and_gives_no_event() {
    let (ctx, mut gallery) = open_input();
    enter(&ctx, &mut gallery);
    let seen = texts(&ctx, &mut gallery, Vec::new());
    assert!(has_text(&seen, "Enter a tag name"), "the Preview text");
    assert_eq!(gallery.take_menu_events(), vec![]);
    assert!(gallery.tag_picker_open(), "the input stayed open");

    texts(&ctx, &mut gallery, vec![Event::Text("red".to_owned())]);
    enter(&ctx, &mut gallery);
    let seen = texts(&ctx, &mut gallery, Vec::new());
    assert!(has_text(&seen, "Name is reserved"));
    assert_eq!(gallery.take_menu_events(), vec![]);
}

#[test]
fn a_valid_new_name_gives_the_event_with_the_trimmed_name() {
    let (ctx, mut gallery) = open_input();
    texts(
        &ctx,
        &mut gallery,
        vec![Event::Text(" fox cub ".to_owned())],
    );
    enter(&ctx, &mut gallery);
    assert_eq!(
        gallery.take_menu_events(),
        vec![GridMenuEvent::TagPaths {
            paths: paths(&["a.png", "b.png"]),
            tag: "fox cub".to_owned(),
        }]
    );
    assert!(!gallery.tag_picker_open(), "the picker closed");
}

#[test]
fn escape_closes_the_picker_with_no_event_and_is_consumed() {
    for input in [false, true] {
        let (ctx, mut gallery) = if input {
            open_input()
        } else {
            open(pos2(300.0, 300.0))
        };
        let mut seen_later = true;
        let raw = test_support::input(900.0, vec![key(Key::Escape)]);
        ctx.run_ui(raw, |ui| {
            gallery.show(ui);
            seen_later = ui.input(|input| input.key_pressed(Key::Escape));
        })
        .drop_without_applying_deltas();
        assert!(!seen_later, "Escape was left for another handler");
        assert!(!gallery.tag_picker_open(), "input {input}");
        assert_eq!(gallery.take_menu_events(), vec![]);
    }
}

#[test]
fn a_press_outside_closes_the_picker_with_no_event() {
    for input in [false, true] {
        let (ctx, mut gallery) = if input {
            open_input()
        } else {
            open(pos2(300.0, 300.0))
        };
        let outside = pos2(1100.0, 700.0);
        for events in test_support::click(outside) {
            texts(&ctx, &mut gallery, vec![events]);
        }
        settle(&ctx, &mut gallery, 2);
        assert!(!gallery.tag_picker_open(), "input {input}");
        assert_eq!(gallery.take_menu_events(), vec![]);
    }
}

#[test]
fn the_picker_acts_on_the_files_it_opened_with() {
    let (ctx, mut gallery) = open(pos2(300.0, 300.0));
    gallery.select_all_visible();
    click_button(&ctx, &mut gallery, "fox");
    assert_eq!(
        gallery.take_menu_events(),
        vec![GridMenuEvent::TagPaths {
            paths: paths(&["a.png", "b.png"]),
            tag: "fox".to_owned(),
        }]
    );
}

#[test]
fn an_empty_list_opens_nothing() {
    let (_ctx, mut gallery) = start();
    gallery.open_tag_picker(pos2(10.0, 10.0), Vec::new());
    assert!(!gallery.tag_picker_open());
}

#[test]
fn the_picker_and_the_dialogs_refuse_each_other() {
    let (_ctx, mut gallery) = open(pos2(300.0, 300.0));
    gallery.open_rename(PathBuf::from("/images/a.png"));
    gallery.open_delete(paths(&["a.png"]), false);
    assert!(!gallery.rename_open(), "Rename opened over the picker");
    assert!(!gallery.delete_open(), "Delete opened over the picker");
    assert!(gallery.tag_picker_open(), "the picker stayed");

    let (_ctx, mut gallery) = start();
    gallery.open_rename(PathBuf::from("/images/a.png"));
    gallery.open_tag_picker(pos2(300.0, 300.0), paths(&["a.png"]));
    assert!(!gallery.tag_picker_open(), "the picker opened over Rename");

    let (_ctx, mut gallery) = start();
    gallery.open_delete(paths(&["a.png"]), false);
    gallery.open_tag_picker(pos2(300.0, 300.0), paths(&["a.png"]));
    assert!(!gallery.tag_picker_open(), "the picker opened over Delete");

    let (_ctx, mut gallery) = open(pos2(300.0, 300.0));
    gallery.open_tag_picker(pos2(10.0, 10.0), paths(&["b.png"]));
    gallery.take_menu_events();
    assert!(gallery.tag_picker_open(), "the first picker stayed");
}

#[test]
fn a_right_click_opens_no_menu_while_the_picker_is_open() {
    let (ctx, mut gallery) = super::context_menu::setup();
    gallery.open_tag_picker(pos2(700.0, 600.0), paths(&["b.png"]));
    let click = CellClick {
        index: 1,
        position: 1,
        modifiers: Modifiers::NONE,
    };
    let (mut tile_opened, mut space_opened) = (false, true);
    let raw = test_support::input(900.0, Vec::new());
    ctx.run_ui(raw, |ui| {
        gallery.open_tile_menu(ui, click);
        tile_opened = !gallery.selected_paths().is_empty();
        space_opened = gallery.open_empty_space_menu(ui);
    })
    .drop_without_applying_deltas();
    assert!(!tile_opened, "the tile menu opened and selected a tile");
    assert!(!space_opened, "the empty-space menu opened");
    for _ in 0..2 {
        let seen = super::context_menu::run(&ctx, &mut gallery, Vec::new());
        assert!(!menu_shows(&seen), "a menu showed under the picker");
    }
}
