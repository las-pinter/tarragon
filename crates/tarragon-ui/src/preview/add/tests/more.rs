//! More tests for the "+ add" control: scrolling, ids, resets, and the cursor.

use super::{
    AddState, CREATE_NEW, Preview, PreviewEvent, click, find, frame, has, open_input, open_menu,
    preview_with, run, setup,
};
use crate::preview::ColorState;
use crate::preview::test_support::{collect, pill, preview};
use egui::{Context, CursorIcon, Event, Key, Modifiers, Pos2, Rect, vec2};

#[test]
fn many_tags_scroll_and_create_new_stays_on_screen() {
    let known: Vec<String> = (0..80).map(|i| format!("tag-{i}")).collect();
    let refs: Vec<&str> = known.iter().map(String::as_str).collect();
    let mut preview = preview_with(vec![], &refs);
    let ctx = setup(&mut preview);
    open_menu(&ctx, &mut preview);
    let texts = frame(&ctx, &mut preview, vec![]);
    let create = find(&texts, CREATE_NEW).expect("Create new... is drawn");
    assert!(create.y + 20.0 < 600.0, "{create:?} is off the screen");
}

#[test]
fn two_previews_in_one_context_keep_separate_add_state() {
    let (mut a, mut b) = (
        preview_with(vec![], &["hands"]),
        preview_with(vec![], &["hands"]),
    );
    let ctx = Context::default();
    crate::theme::apply_theme(&ctx);
    let both = |ctx: &Context, a: &mut Preview, b: &mut Preview, events: Vec<Event>| {
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0))),
            events,
            ..Default::default()
        };
        let mut texts = Vec::new();
        let output = ctx.run_ui(input, |ui| {
            ui.columns(2, |cols| {
                cols[0].push_id("a", |ui| a.show(ui));
                cols[1].push_id("b", |ui| b.show(ui));
            });
        });
        output
            .shapes
            .iter()
            .for_each(|c| collect(&c.shape, &mut texts));
        output.drop_without_applying_deltas();
        texts
    };
    both(&ctx, &mut a, &mut b, vec![]);
    both(&ctx, &mut a, &mut b, vec![]);
    let adds = |texts: &[(String, Pos2)]| -> Vec<Pos2> {
        texts
            .iter()
            .filter(|(t, _)| t == "+ add")
            .map(|(_, p)| *p)
            .collect()
    };
    let buttons = adds(&both(&ctx, &mut a, &mut b, vec![]));
    assert_eq!(buttons.len(), 2);
    let left = buttons[0] + vec2(3.0, 3.0);
    both(&ctx, &mut a, &mut b, vec![Event::PointerMoved(left)]);
    both(&ctx, &mut a, &mut b, click(left));
    let texts = both(&ctx, &mut a, &mut b, vec![]);
    assert_eq!(
        texts.iter().filter(|(t, _)| t == "hands").count(),
        1,
        "only A is open"
    );

    let right = buttons[1] + vec2(3.0, 3.0);
    both(&ctx, &mut a, &mut b, vec![Event::PointerMoved(right)]);
    both(&ctx, &mut a, &mut b, click(right));
    let texts = both(&ctx, &mut a, &mut b, vec![]);
    // The press on B's button is outside A's menu, so A closes and B opens.
    let rows: Vec<Pos2> = texts
        .iter()
        .filter(|(t, _)| t == "hands")
        .map(|(_, p)| *p)
        .collect();
    assert_eq!(rows.len(), 1, "one menu is open");
    assert!(
        rows[0].x > 400.0,
        "{rows:?}: B's menu sits under B's button"
    );
}

#[test]
fn selection_changed_closes_the_menu_and_the_input_but_set_tags_does_not() {
    let mut preview = preview_with(vec![], &["hands"]);
    let ctx = setup(&mut preview);
    open_menu(&ctx, &mut preview);
    preview.set_tags(vec![pill("hands", ColorState::Some)]);
    assert!(
        has(&frame(&ctx, &mut preview, vec![]), CREATE_NEW),
        "set_tags keeps it open"
    );
    preview.selection_changed();
    assert!(!has(&frame(&ctx, &mut preview, vec![]), CREATE_NEW));

    open_input(&ctx, &mut preview);
    preview.selection_changed();
    let texts = frame(&ctx, &mut preview, vec![]);
    assert!(has(&texts, "+ add") && !has(&texts, "Tag name..."));
}

#[test]
fn drawing_the_empty_hint_resets_a_hidden_input() {
    let mut preview = preview_with(vec![], &[]);
    let ctx = setup(&mut preview);
    open_input(&ctx, &mut preview);
    preview.set_meta(None);
    assert!(has(
        &frame(&ctx, &mut preview, vec![]),
        crate::preview::EMPTY_HINT
    ));
    preview.set_meta(Some(crate::preview::Meta {
        name: "b.png".into(),
        dimensions: None,
        size: None,
        format: "PNG".into(),
    }));
    let texts = frame(&ctx, &mut preview, vec![]);
    assert!(
        has(&texts, "+ add"),
        "the button is back, not a stale input"
    );
}

#[test]
fn a_press_outside_the_open_input_closes_it_with_no_event() {
    let mut preview = preview_with(vec![], &[]);
    let ctx = setup(&mut preview);
    open_input(&ctx, &mut preview);
    frame(&ctx, &mut preview, vec![Event::Text("blue".into())]);
    frame(&ctx, &mut preview, click(Pos2::new(380.0, 580.0)));
    assert!(has(&frame(&ctx, &mut preview, vec![]), "+ add"));
    assert_eq!(preview.take_events(), Vec::<PreviewEvent>::new());
}

#[test]
fn a_press_in_the_menu_frame_but_off_every_row_keeps_it_open() {
    let mut preview = preview_with(vec![], &["hands"]);
    let ctx = setup(&mut preview);
    open_menu(&ctx, &mut preview);
    let texts = frame(&ctx, &mut preview, vec![]);
    let row = find(&texts, CREATE_NEW).unwrap();
    // Left of the row text lies the row padding, then the frame margin.
    let margin = Pos2::new(row.x - crate::tokens::SPACING_S - 2.0, row.y + 3.0);
    frame(&ctx, &mut preview, click(margin));
    assert!(has(&frame(&ctx, &mut preview, vec![]), CREATE_NEW));
    assert_eq!(preview.take_events(), Vec::<PreviewEvent>::new());
}

#[test]
fn the_button_and_the_menu_rows_show_the_hand_cursor() {
    let mut preview = preview(vec![]);
    preview.set_known_tags(vec!["hands".into()]);
    let ctx = setup(&mut preview);
    let texts = frame(&ctx, &mut preview, vec![]);
    let button = find(&texts, "+ add").unwrap() + vec2(3.0, 3.0);
    frame(&ctx, &mut preview, vec![Event::PointerMoved(button)]);
    assert_eq!(run(&ctx, &mut preview, vec![]).1, CursorIcon::PointingHand);

    frame(
        &ctx,
        &mut preview,
        vec![Event::PointerMoved(Pos2::new(390.0, 590.0))],
    );
    open_menu(&ctx, &mut preview);
    let texts = frame(&ctx, &mut preview, vec![]);
    let row = find(&texts, "hands").unwrap() + vec2(3.0, 3.0);
    frame(&ctx, &mut preview, vec![Event::PointerMoved(row)]);
    assert_eq!(run(&ctx, &mut preview, vec![]).1, CursorIcon::PointingHand);
}

/// The text the open input holds, or `None` when it is not open.
fn input_text(preview: &Preview) -> Option<&str> {
    match &preview.add {
        AddState::Input { text, .. } => Some(text),
        _ => None,
    }
}

/// Run one frame at `time` seconds. egui records undo points only after the
/// text has been stable for a while, so the undo test passes simulated time.
fn frame_at(ctx: &Context, preview: &mut Preview, time: f64, events: Vec<Event>) {
    let input = egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(400.0, 600.0))),
        time: Some(time),
        events,
        ..Default::default()
    };
    ctx.run_ui(input, |ui| preview.show(ui))
        .drop_without_applying_deltas();
}

#[test]
fn undo_in_a_reopened_input_does_not_bring_back_old_text() {
    let mut preview = preview_with(vec![], &[]);
    let ctx = setup(&mut preview);
    open_input(&ctx, &mut preview);
    frame_at(&ctx, &mut preview, 10.0, vec![]);
    frame_at(&ctx, &mut preview, 11.0, vec![Event::Text("old".into())]);
    frame_at(&ctx, &mut preview, 15.0, vec![]);
    frame_at(&ctx, &mut preview, 20.0, vec![]);
    assert_eq!(input_text(&preview), Some("old"));
    preview.selection_changed();
    frame_at(&ctx, &mut preview, 21.0, vec![]);
    open_input(&ctx, &mut preview);
    let undo = Event::Key {
        key: Key::Z,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::COMMAND,
    };
    frame_at(&ctx, &mut preview, 30.0, vec![undo]);
    frame_at(&ctx, &mut preview, 31.0, vec![]);
    assert_eq!(input_text(&preview), Some(""));
}

#[test]
fn a_very_long_tag_name_keeps_the_menu_inside_the_pane() {
    let long = "x".repeat(300);
    let mut preview = preview_with(vec![], &[long.as_str()]);
    let ctx = setup(&mut preview);
    open_menu(&ctx, &mut preview);
    frame(&ctx, &mut preview, vec![]);
    assert!(has(&frame(&ctx, &mut preview, vec![]), CREATE_NEW));
    let menu = ctx.memory(|m| {
        m.areas().visible_layer_ids().into_iter().find_map(|layer| {
            (layer.order == egui::Order::Foreground)
                .then(|| m.area_rect(layer.id))
                .flatten()
        })
    });
    let menu = menu.expect("the menu is an area");
    assert!(menu.max.x <= 400.0, "the menu reaches x = {}", menu.max.x);
}

#[test]
fn a_very_narrow_pane_still_opens_the_menu_with_a_non_negative_width() {
    let mut preview = preview_with(vec![], &["hands"]);
    let ctx = Context::default();
    crate::theme::apply_theme(&ctx);
    let narrow = |ctx: &Context, preview: &mut Preview, events: Vec<Event>| {
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(60.0, 600.0))),
            events,
            ..Default::default()
        };
        let mut texts = Vec::new();
        let output = ctx.run_ui(input, |ui| preview.show(ui));
        output
            .shapes
            .iter()
            .for_each(|c| collect(&c.shape, &mut texts));
        output.drop_without_applying_deltas();
        texts
    };
    narrow(&ctx, &mut preview, vec![]);
    let texts = narrow(&ctx, &mut preview, vec![]);
    let at = find(&texts, "+ add").expect("the button is drawn") + vec2(3.0, 3.0);
    narrow(&ctx, &mut preview, vec![Event::PointerMoved(at)]);
    narrow(&ctx, &mut preview, click(at));
    narrow(&ctx, &mut preview, vec![]);
    let menu = ctx.memory(|m| {
        m.areas().visible_layer_ids().into_iter().find_map(|layer| {
            (layer.order == egui::Order::Foreground)
                .then(|| m.area_rect(layer.id))
                .flatten()
        })
    });
    let menu = menu.expect("the menu is open");
    assert!(menu.width() >= 0.0, "{menu:?}");
}
