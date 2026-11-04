//! Tests for the crash card.

use super::{CrashCard, CrashCardEvent, CrashReport};
use crate::{Shell, test_support};
use egui::epaint::Shape;
use egui::{Context, Event, Key, Modifiers, Pos2, Rect};

fn report() -> CrashReport {
    CrashReport {
        file_name: "crash-20261104T101500Z.txt".to_owned(),
        path: "/data/crashes/crash-20261104T101500Z.txt".to_owned(),
        summary: "tarragon 0.1.0\nmessage: index out of bounds".to_owned(),
    }
}

fn escape() -> Event {
    Event::Key {
        key: Key::Escape,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::NONE,
    }
}

fn collect(shape: &Shape, found: &mut Vec<(String, Rect)>) {
    match shape {
        Shape::Vec(shapes) => shapes.iter().for_each(|s| collect(s, found)),
        Shape::Text(t) => {
            let rect = t.galley.rect.translate(t.pos.to_vec2());
            found.push((t.galley.job.text.clone(), rect));
        }
        _ => {}
    }
}

/// Run one frame with a pane button at the top-left, then the card. Return
/// whether the pane button was clicked and every painted text with its rect.
fn frame(ctx: &Context, card: &mut CrashCard, events: Vec<Event>) -> (bool, Vec<(String, Rect)>) {
    let mut clicked = false;
    let output = ctx.run_ui(test_support::input(600.0, events), |ui| {
        clicked = ui.button("pane").clicked();
        card.show(ui.ctx());
    });
    let mut found = Vec::new();
    for clipped in &output.shapes {
        collect(&clipped.shape, &mut found);
    }
    output.drop_without_applying_deltas();
    (clicked, found)
}

/// An open card, drawn for two frames: an egui window is hidden in its first.
fn open_card() -> (Context, CrashCard) {
    let ctx = test_support::context();
    let mut card = CrashCard::default();
    card.show_report(report());
    frame(&ctx, &mut card, vec![]);
    frame(&ctx, &mut card, vec![]);
    (ctx, card)
}

fn centre_of(texts: &[(String, Rect)], want: &str) -> Pos2 {
    let (_, rect) = texts
        .iter()
        .find(|(text, _)| text == want)
        .unwrap_or_else(|| panic!("no text {want:?} in {texts:?}"));
    rect.center()
}

#[test]
fn a_frame_shows_the_title_the_path_and_the_summary() {
    let (ctx, mut card) = open_card();
    let (_, texts) = frame(&ctx, &mut card, vec![]);
    let has = |want: &str| texts.iter().any(|(text, _)| text == want);
    assert!(has("Tarragon closed unexpectedly"), "{texts:?}");
    assert!(has(&report().path), "{texts:?}");
    assert!(has(&report().summary), "{texts:?}");
}

#[test]
fn dismiss_queues_an_event_and_closes_the_card() {
    let (ctx, mut card) = open_card();
    let (_, texts) = frame(&ctx, &mut card, vec![]);
    let at = centre_of(&texts, "Dismiss");
    frame(&ctx, &mut card, test_support::click(at));
    assert_eq!(card.take_events(), vec![CrashCardEvent::Dismiss]);
    assert!(!card.is_open());
}

#[test]
fn open_report_queues_an_event_and_keeps_the_card_open() {
    let (ctx, mut card) = open_card();
    let (_, texts) = frame(&ctx, &mut card, vec![]);
    let at = centre_of(&texts, "Open report");
    frame(&ctx, &mut card, test_support::click(at));
    assert_eq!(card.take_events(), vec![CrashCardEvent::OpenReport]);
    assert!(card.is_open());
}

#[test]
fn escape_queues_dismiss_and_closes_the_card() {
    let (ctx, mut card) = open_card();
    frame(&ctx, &mut card, vec![escape()]);
    assert_eq!(card.take_events(), vec![CrashCardEvent::Dismiss]);
    assert!(!card.is_open());
}

#[test]
fn an_unread_dismiss_does_not_close_the_next_report() {
    let (ctx, mut card) = open_card();
    frame(&ctx, &mut card, vec![escape()]);
    // The caller has not taken the Dismiss yet.
    card.show_report(report());
    frame(&ctx, &mut card, vec![]);
    assert!(card.is_open());
}

#[test]
fn a_click_does_not_reach_the_pane_while_the_card_is_open() {
    let (ctx, mut card) = open_card();
    let at = Pos2::new(16.0, 16.0);
    let (hit, _) = frame(&ctx, &mut card, test_support::click(at));
    assert!(!hit, "the pane took a click through the dim layer");
    assert!(card.is_open());

    // Control: the same click lands once the card is closed.
    frame(&ctx, &mut card, vec![escape()]);
    frame(&ctx, &mut card, vec![]);
    let (hit, _) = frame(&ctx, &mut card, test_support::click(at));
    assert!(hit, "the pane button is not at the clicked point");
}

#[test]
fn tab_does_not_move_focus_to_the_pane_while_the_card_is_open() {
    let (ctx, mut card) = open_card();
    let tab = Event::Key {
        key: Key::Tab,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::NONE,
    };
    let mut pane_has_focus = false;
    for _ in 0..3 {
        let mut id = None;
        let output = ctx.run_ui(test_support::input(600.0, vec![tab.clone()]), |ui| {
            id = Some(ui.button("pane").id);
            card.show(ui.ctx());
        });
        output.drop_without_applying_deltas();
        pane_has_focus |= ctx.memory(|m| m.has_focus(id.expect("the pane button")));
    }
    assert!(
        !pane_has_focus,
        "Tab reached the pane through the modal layer"
    );
}

#[test]
fn with_no_report_nothing_is_drawn_and_the_card_is_closed() {
    let ctx = test_support::context();
    let mut card = CrashCard::default();
    assert!(!card.is_open());
    let (_, texts) = frame(&ctx, &mut card, vec![]);
    assert_eq!(texts.len(), 1, "only the pane button text: {texts:?}");
    assert_eq!(card.take_events().len(), 0);
}

/// Run one shell frame and return every painted text.
fn shell_frame(ctx: &Context, shell: &mut Shell, events: Vec<Event>) -> Vec<(String, Rect)> {
    let output = ctx.run_ui(test_support::input(1400.0, events), |ui| shell.show(ui));
    let mut found = Vec::new();
    for clipped in &output.shapes {
        collect(&clipped.shape, &mut found);
    }
    output.drop_without_applying_deltas();
    found
}

#[test]
fn the_shell_draws_the_card_and_hands_back_its_events() {
    let ctx = test_support::context();
    let mut shell = Shell::new();
    shell.show_crash_report(report());
    shell_frame(&ctx, &mut shell, vec![]);
    let texts = shell_frame(&ctx, &mut shell, vec![]);
    assert!(
        texts
            .iter()
            .any(|(t, _)| t == "Tarragon closed unexpectedly")
    );
    assert_eq!(shell.take_crash_events().len(), 0);

    shell_frame(&ctx, &mut shell, vec![escape()]);
    assert_eq!(shell.take_crash_events(), vec![CrashCardEvent::Dismiss]);
}
