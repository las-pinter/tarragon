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
    /// Filled rects with a visible border: the menu frame.
    frames: Vec<Rect>,
}

fn walk(shape: &Shape, f: &mut Frame) {
    match shape {
        Shape::Vec(v) => v.iter().for_each(|s| walk(s, f)),
        Shape::Rect(r) if r.fill.a() > 0 && r.stroke.color.a() > 0 => f.frames.push(r.rect),
        Shape::Text(t) => f.texts.push((t.galley.job.text.clone(), t.pos)),
        Shape::Path(_) => f.ticks += 1,
        _ => {}
    }
}

fn run(ctx: &Context, at: Rect, list: &[(String, bool)], events: Vec<Event>) -> Frame {
    run_with(ctx, at, list, None, events)
}

fn run_with(
    ctx: &Context,
    at: Rect,
    list: &[(String, bool)],
    footer: Option<&str>,
    events: Vec<Event>,
) -> Frame {
    let mut result = MenuResult::Open;
    let output = ctx.run_ui(input(400.0, events), |ui| {
        result = show_check_menu(ui, Id::new("m"), at, list, footer);
    });
    let mut f = Frame {
        result,
        texts: vec![],
        ticks: 0,
        frames: vec![],
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

const FOOTER: &str = "Manage tags...";

/// The one menu frame of the second frame, once the fade-in is done.
fn menu_frame(list: &[(String, bool)], footer: Option<&str>) -> Rect {
    let ctx = context();
    run_with(&ctx, button(50.0), list, footer, vec![]);
    let f = run_with(&ctx, button(50.0), list, footer, vec![]);
    assert_eq!(f.frames.len(), 1, "{:?}", f.frames);
    f.frames[0]
}

#[test]
fn a_footer_wider_than_every_row_widens_the_menu() {
    let long = "Manage every tag in the whole library...";
    let ctx = context();
    run_with(&ctx, button(50.0), &rows(3), Some(long), vec![]);
    let f = run_with(&ctx, button(50.0), &rows(3), Some(long), vec![]);
    let mut text_width = 0.0;
    ctx.run_ui(input(400.0, vec![]), |ui| {
        text_width = crate::popup::text_width(ui, long);
    })
    .drop_without_applying_deltas();
    assert_eq!(f.frames.len(), 1, "{:?}", f.frames);
    assert!(pos_of(&f, long).x + text_width <= f.frames[0].right());
}

#[test]
fn the_drawn_footer_menu_is_as_tall_as_its_height() {
    for count in [0, 3, 30] {
        let frame = menu_frame(&rows(count), Some(FOOTER));
        let want = super::layout::menu_height(count) + super::layout::FOOTER_HEIGHT;
        assert_eq!(frame.height(), want, "{count} rows");
    }
}

#[test]
fn the_footer_stays_below_the_scroll_area_with_no_rows_and_with_many() {
    for count in [0, 30] {
        let ctx = context();
        let list = rows(count);
        run_with(&ctx, button(50.0), &list, Some(FOOTER), vec![]);
        let f = run_with(&ctx, button(50.0), &list, Some(FOOTER), vec![]);
        let footer = pos_of(&f, FOOTER);
        let first = f.texts.iter().find(|(t, _)| t == "row-0");
        assert!(first.is_none_or(|(_, p)| p.y < footer.y), "{count} rows");
        let reach = (super::layout::MENU_ROWS + 2.0) * tokens::MENU_ROW_HEIGHT;
        assert!(footer.y - 50.0 < reach, "{count} rows");
        assert_eq!(f.texts.iter().any(|(t, _)| t == "No items"), count == 0);
        assert_eq!(f.ticks, usize::from(count > 0));
    }
}

#[test]
fn no_footer_draws_no_extra_row() {
    let ctx = context();
    let list = rows(3);
    run(&ctx, button(50.0), &list, vec![]);
    let f = run(&ctx, button(50.0), &list, vec![]);
    assert_eq!(f.texts.len(), 3);
}

#[test]
fn a_footer_click_reports_the_footer_and_no_row() {
    let ctx = context();
    let list = rows(3);
    run_with(&ctx, button(50.0), &list, Some(FOOTER), vec![]);
    let f = run_with(&ctx, button(50.0), &list, Some(FOOTER), vec![]);
    let at = pos_of(&f, FOOTER) + vec2(4.0, 6.0);
    run_with(
        &ctx,
        button(50.0),
        &list,
        Some(FOOTER),
        vec![Event::PointerMoved(at)],
    );
    let f = run_with(&ctx, button(50.0), &list, Some(FOOTER), click(at));
    assert_eq!(f.result, MenuResult::Footer);
}

#[test]
fn escape_closes_a_menu_with_a_footer() {
    let ctx = context();
    let list = rows(3);
    run_with(&ctx, button(50.0), &list, Some(FOOTER), vec![]);
    let esc = Event::Key {
        key: egui::Key::Escape,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::NONE,
    };
    let f = run_with(&ctx, button(50.0), &list, Some(FOOTER), vec![esc]);
    assert_eq!(f.result, MenuResult::Closed);
}

#[test]
fn the_footer_flips_the_menu_above_sooner() {
    let ctx = context();
    let list = rows(3);
    let at = button(260.0);
    run(&ctx, at, &list, vec![]);
    let plain = run(&ctx, at, &list, vec![]);
    run_with(&ctx, at, &list, Some(FOOTER), vec![]);
    let with = run_with(&ctx, at, &list, Some(FOOTER), vec![]);
    assert!(pos_of(&plain, "row-0").y > at.max.y);
    assert!(pos_of(&with, "row-0").y < at.min.y);
}
