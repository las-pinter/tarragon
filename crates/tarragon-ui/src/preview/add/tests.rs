//! Tests for the "+ add" control.

use super::{AddState, CREATE_NEW};
use crate::preview::test_support::{collect, pill, preview};
use crate::preview::{ColorState, Preview, PreviewEvent, TagPill};
use egui::{Context, Event, Key, Modifiers, PointerButton, Pos2, Rect, vec2};

/// A Preview with known tags and no colour squares, so the row is the pills.
fn preview_with(tags: Vec<TagPill>, known: &[&str]) -> Preview {
    let mut preview = preview(tags);
    preview.set_known_tags(known.iter().map(|s| (*s).to_owned()).collect());
    preview.set_colors_enabled(false);
    preview
}

/// A context with the theme fonts, after two frames so text has a real size.
fn setup(preview: &mut Preview) -> Context {
    let ctx = Context::default();
    crate::theme::apply_theme(&ctx);
    frame(&ctx, preview, vec![]);
    frame(&ctx, preview, vec![]);
    ctx
}

/// Run one frame; return every painted text and its position.
fn frame(ctx: &Context, preview: &mut Preview, events: Vec<Event>) -> Vec<(String, Pos2)> {
    run(ctx, preview, events).0
}

/// Run one frame; also return the cursor icon the frame asked for.
fn run(
    ctx: &Context,
    preview: &mut Preview,
    events: Vec<Event>,
) -> (Vec<(String, Pos2)>, egui::CursorIcon) {
    let input = egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(400.0, 600.0))),
        events,
        ..Default::default()
    };
    let output = ctx.run_ui(input, |ui| preview.show(ui));
    let mut found = Vec::new();
    for clipped in &output.shapes {
        collect(&clipped.shape, &mut found);
    }
    let cursor = output.platform_output.cursor_icon;
    output.drop_without_applying_deltas();
    (found, cursor)
}

fn find(texts: &[(String, Pos2)], want: &str) -> Option<Pos2> {
    texts.iter().find(|(t, _)| t == want).map(|(_, p)| *p)
}

fn has(texts: &[(String, Pos2)], want: &str) -> bool {
    find(texts, want).is_some()
}

fn click(at: Pos2) -> Vec<Event> {
    let button = |pressed| Event::PointerButton {
        pos: at,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    };
    vec![Event::PointerMoved(at), button(true), button(false)]
}

fn key(key: Key) -> Event {
    Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::NONE,
    }
}

/// Click the text `label`, which must be on screen. The pointer first moves
/// there in its own frame, because egui hit-tests against the last layout.
fn click_text(ctx: &Context, preview: &mut Preview, label: &str) {
    let texts = frame(ctx, preview, vec![]);
    let at = find(&texts, label).unwrap_or_else(|| panic!("{label} is not drawn")) + vec2(3.0, 3.0);
    frame(ctx, preview, vec![Event::PointerMoved(at)]);
    frame(ctx, preview, click(at));
}

/// Open the menu and let it paint.
fn open_menu(ctx: &Context, preview: &mut Preview) {
    click_text(ctx, preview, "+ add");
    frame(ctx, preview, vec![]);
}

/// Open the name input and let it take focus.
fn open_input(ctx: &Context, preview: &mut Preview) {
    open_menu(ctx, preview);
    click_text(ctx, preview, CREATE_NEW);
    frame(ctx, preview, vec![]);
    frame(ctx, preview, vec![]);
}

#[test]
fn the_add_button_paints_below_the_tag_pills_and_also_with_no_tags() {
    let mut empty = preview_with(vec![], &[]);
    let ctx = setup(&mut empty);
    assert!(has(&frame(&ctx, &mut empty, vec![]), "+ add"));

    let mut full = preview_with(
        vec![pill("one", ColorState::All), pill("two", ColorState::Some)],
        &[],
    );
    let ctx = setup(&mut full);
    let texts = frame(&ctx, &mut full, vec![]);
    let (two, add) = (find(&texts, "two").unwrap(), find(&texts, "+ add").unwrap());
    assert!(add.y > two.y + 20.0 && add.x <= two.x, "{two:?} {add:?}");
}

#[test]
fn a_click_on_the_button_opens_the_menu_and_a_second_click_closes_it() {
    let mut preview = preview_with(vec![], &["hands"]);
    let ctx = setup(&mut preview);
    assert!(!has(&frame(&ctx, &mut preview, vec![]), "hands"));
    open_menu(&ctx, &mut preview);
    let texts = frame(&ctx, &mut preview, vec![]);
    assert!(has(&texts, "hands") && has(&texts, CREATE_NEW));
    click_text(&ctx, &mut preview, "+ add");
    assert!(!has(&frame(&ctx, &mut preview, vec![]), "hands"));
    assert_eq!(preview.take_events(), Vec::<PreviewEvent>::new());
}

#[test]
fn choosing_a_listed_tag_queues_one_apply_and_closes_the_menu() {
    let mut preview = preview_with(vec![], &["hands", "todo"]);
    let ctx = setup(&mut preview);
    open_menu(&ctx, &mut preview);
    click_text(&ctx, &mut preview, "todo");
    assert_eq!(
        preview.take_events(),
        [PreviewEvent::TagApplied("todo".into())]
    );
    let texts = frame(&ctx, &mut preview, vec![]);
    assert!(!has(&texts, "hands") && !has(&texts, CREATE_NEW));
}

#[test]
fn the_menu_omits_fully_applied_tags_and_keeps_partial_ones() {
    let mut preview = preview_with(
        vec![pill("all", ColorState::All), pill("some", ColorState::Some)],
        &["all", "some", "other"],
    );
    let ctx = setup(&mut preview);
    open_menu(&ctx, &mut preview);
    let texts = frame(&ctx, &mut preview, vec![]);
    // A pill and a menu row share a name, so count the paints of each.
    let count = |name: &str| texts.iter().filter(|(t, _)| t == name).count();
    assert_eq!(count("all"), 1, "only the pill");
    assert_eq!(count("some"), 2, "the pill and the row");
    assert_eq!(count("other"), 1, "the row");
}

#[test]
fn enter_applies_the_trimmed_name() {
    let mut preview = preview_with(vec![], &[]);
    let ctx = setup(&mut preview);
    open_input(&ctx, &mut preview);
    frame(&ctx, &mut preview, vec![Event::Text("  blue  ".into())]);
    frame(&ctx, &mut preview, vec![key(Key::Enter)]);
    assert_eq!(
        preview.take_events(),
        [PreviewEvent::TagApplied("blue".into())]
    );
    assert!(
        has(&frame(&ctx, &mut preview, vec![]), "+ add"),
        "button is back"
    );
}

#[test]
fn enter_on_a_blank_name_queues_nothing_and_closes_the_input() {
    let mut preview = preview_with(vec![], &[]);
    let ctx = setup(&mut preview);
    open_input(&ctx, &mut preview);
    frame(&ctx, &mut preview, vec![Event::Text("   ".into())]);
    frame(&ctx, &mut preview, vec![key(Key::Enter)]);
    assert_eq!(preview.take_events(), Vec::<PreviewEvent>::new());
    assert!(has(&frame(&ctx, &mut preview, vec![]), "+ add"));
}

#[test]
fn escape_closes_the_menu_and_the_input_with_no_event() {
    let mut preview = preview_with(vec![], &["hands"]);
    let ctx = setup(&mut preview);
    open_menu(&ctx, &mut preview);
    frame(&ctx, &mut preview, vec![key(Key::Escape)]);
    assert!(!has(&frame(&ctx, &mut preview, vec![]), "hands"));

    open_input(&ctx, &mut preview);
    frame(&ctx, &mut preview, vec![Event::Text("blue".into())]);
    frame(&ctx, &mut preview, vec![key(Key::Escape)]);
    assert!(has(&frame(&ctx, &mut preview, vec![]), "+ add"));
    assert_eq!(preview.take_events(), Vec::<PreviewEvent>::new());
}

#[test]
fn a_press_outside_the_menu_closes_it_with_no_event() {
    let mut preview = preview_with(vec![], &["hands"]);
    let ctx = setup(&mut preview);
    open_menu(&ctx, &mut preview);
    frame(&ctx, &mut preview, click(Pos2::new(380.0, 580.0)));
    assert!(!has(&frame(&ctx, &mut preview, vec![]), "hands"));
    assert_eq!(preview.take_events(), Vec::<PreviewEvent>::new());
}

#[test]
fn the_default_state_is_closed() {
    assert_eq!(AddState::default(), AddState::Closed);
}

mod layout;
mod more;
