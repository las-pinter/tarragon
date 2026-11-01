//! Tests for the check menu, run headless.

use super::{MenuResult, show_check_menu};
use crate::test_support::{context, input};
use crate::tokens;
use egui::epaint::Shape;
use egui::{Context, Event, Id, Modifiers, PointerButton, Pos2, Rect, vec2};

fn rows(n: usize) -> Vec<(String, bool)> {
    (0..n).map(|i| (format!("row-{i}"), i == 0)).collect()
}

fn button(y: f32) -> Rect {
    Rect::from_min_size(Pos2::new(20.0, y), vec2(80.0, 24.0))
}

/// What one frame painted: the result, text positions, and tick paths.
struct Frame {
    result: MenuResult,
    texts: Vec<(String, Pos2)>,
    ticks: usize,
}

fn walk(shape: &Shape, f: &mut Frame) {
    match shape {
        Shape::Vec(v) => v.iter().for_each(|s| walk(s, f)),
        Shape::Text(t) => f.texts.push((t.galley.job.text.clone(), t.pos)),
        Shape::Path(_) => f.ticks += 1,
        _ => {}
    }
}

fn run(ctx: &Context, at: Rect, list: &[(String, bool)], events: Vec<Event>) -> Frame {
    let mut result = MenuResult::Open;
    let output = ctx.run_ui(input(400.0, events), |ui| {
        result = show_check_menu(ui, Id::new("m"), at, list);
    });
    let mut f = Frame {
        result,
        texts: vec![],
        ticks: 0,
    };
    output.shapes.iter().for_each(|c| walk(&c.shape, &mut f));
    output.drop_without_applying_deltas();
    f
}

fn click(pos: Pos2) -> Vec<Event> {
    [true, false]
        .map(|pressed| Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        })
        .into()
}

fn pos_of(f: &Frame, text: &str) -> Pos2 {
    f.texts
        .iter()
        .find(|(t, _)| t == text)
        .map(|(_, p)| *p)
        .expect("text is drawn")
}

#[test]
fn a_checked_row_shows_a_tick_and_an_unchecked_row_keeps_the_blank() {
    let ctx = context();
    let list = vec![("on".to_owned(), true), ("off".to_owned(), false)];
    run(&ctx, button(50.0), &list, vec![]);
    let f = run(&ctx, button(50.0), &list, vec![]);
    assert_eq!(f.ticks, 1);
    assert_eq!(pos_of(&f, "on").x, pos_of(&f, "off").x);
    assert!(pos_of(&f, "off").x >= 20.0 + tokens::MENU_ROW_PAD_X + 12.0);
}

#[test]
fn a_row_click_toggles_and_the_menu_stays_open() {
    let ctx = context();
    let list = rows(3);
    run(&ctx, button(50.0), &list, vec![]);
    let f = run(&ctx, button(50.0), &list, vec![]);
    let at = pos_of(&f, "row-1") + vec2(4.0, 6.0);
    run(&ctx, button(50.0), &list, vec![Event::PointerMoved(at)]);
    let f = run(&ctx, button(50.0), &list, click(at));
    assert_eq!(f.result, MenuResult::Toggled(1));
}

#[test]
fn an_outside_click_or_escape_closes() {
    let ctx = context();
    let list = rows(3);
    run(&ctx, button(50.0), &list, vec![]);
    let f = run(&ctx, button(50.0), &list, click(Pos2::new(390.0, 390.0)));
    assert_eq!(f.result, MenuResult::Closed);
    let esc = Event::Key {
        key: egui::Key::Escape,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::NONE,
    };
    assert_eq!(
        run(&ctx, button(50.0), &list, vec![esc]).result,
        MenuResult::Closed
    );
}

#[test]
fn the_menu_flips_above_at_the_bottom_edge() {
    let ctx = context();
    let list = rows(3);
    run(&ctx, button(370.0), &list, vec![]);
    let f = run(&ctx, button(370.0), &list, vec![]);
    assert!(pos_of(&f, "row-0").y < 370.0);
}

#[test]
fn an_empty_list_shows_a_no_items_row() {
    let ctx = context();
    run(&ctx, button(50.0), &[], vec![]);
    let f = run(&ctx, button(50.0), &[], vec![]);
    pos_of(&f, "No items");
    assert_eq!(f.ticks, 0);
}

#[test]
fn a_click_on_the_button_leaves_the_menu_open() {
    let ctx = context();
    let list = rows(3);
    run(&ctx, button(50.0), &list, vec![]);
    let f = run(&ctx, button(50.0), &list, click(button(50.0).center()));
    assert_eq!(f.result, MenuResult::Open);
}

#[test]
fn the_height_stops_growing_at_twelve_rows() {
    assert_eq!(
        super::layout::menu_height(20),
        super::layout::menu_height(12)
    );
}
