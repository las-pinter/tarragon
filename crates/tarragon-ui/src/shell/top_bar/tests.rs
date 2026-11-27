//! Tests for the top bar.

use std::path::PathBuf;
use std::time::Duration;

use egui::{Context, Event, Id, Modifiers, PointerButton, Pos2};

use super::*;
use crate::gallery::SEARCH_DEBOUNCE;
use crate::grid::GridItem;
use crate::shell::MenuEvent;
use crate::shell::tests::{draw, input, painted_text};
use crate::theme::apply_theme;

/// The 1400 by 900 screen that the shell tests use, with `events`.
fn screen(events: Vec<Event>) -> egui::RawInput {
    let mut input = input();
    input.events = events;
    input
}

/// The centre of the widget with `id`, from the last frame.
fn centre(ctx: &Context, id: Id) -> Pos2 {
    ctx.read_response(id)
        .expect("the widget registered a response")
        .rect
        .center()
}

/// A primary press and release at `pos`.
fn click(pos: Pos2) -> Vec<Event> {
    let button = |pressed| Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    };
    vec![Event::PointerMoved(pos), button(true), button(false)]
}

/// Draw one frame with `events`.
fn frame(ctx: &Context, shell: &mut Shell, events: Vec<Event>) {
    ctx.run_ui(screen(events), |ui| shell.show(ui))
        .drop_without_applying_deltas();
}

#[test]
fn the_bar_paints_the_logo_the_wordmark_the_menus_and_the_search_hint() {
    let ctx = Context::default();
    apply_theme(&ctx);
    let mut shell = Shell::new();
    draw(&ctx, &mut shell);
    let output = ctx.run_ui(input(), |ui| shell.show(ui));
    let text = painted_text(&output);
    for label in ["Tarragon", "File", "View", "Settings"] {
        assert!(text.contains(&label.to_owned()), "{label} is not painted");
    }
    assert!(
        text.contains(&SEARCH_HINT.to_owned()),
        "the search hint is not painted: {text:?}"
    );
    assert!(
        text.contains(&SHORTCUT_HINT.to_owned()),
        "the shortcut chip is not painted: {text:?}"
    );
    // The logo is the coral square of the redesign, 22 points with a 5 point
    // radius.
    let logo = output
        .shapes
        .iter()
        .find_map(|clipped| match &clipped.shape {
            egui::epaint::Shape::Rect(rect) if rect.fill == tokens::CORAL_BRIGHT => Some(rect.rect),
            _ => None,
        });
    let logo = logo.expect("the coral logo is not painted");
    assert!(
        (logo.width() - LOGO_SIZE).abs() < 0.5 && (logo.height() - LOGO_SIZE).abs() < 0.5,
        "the logo is {logo:?}"
    );
    output.drop_without_applying_deltas();
}

#[test]
fn the_log_button_toggles_the_log_panel() {
    let ctx = Context::default();
    apply_theme(&ctx);
    let mut shell = Shell::new();
    draw(&ctx, &mut shell);
    assert!(!shell.log_visible(), "the log starts hidden");
    let log = centre(&ctx, Id::new(("tarragon-icon-button", "Show log")));

    frame(&ctx, &mut shell, click(log));

    assert!(shell.log_visible(), "the log button shows the panel");
}

#[test]
fn the_settings_button_opens_preferences() {
    let ctx = Context::default();
    apply_theme(&ctx);
    let mut shell = Shell::new();
    draw(&ctx, &mut shell);
    let settings = centre(&ctx, Id::new(("tarragon-icon-button", "Settings")));

    frame(&ctx, &mut shell, click(settings));

    assert_eq!(shell.take_menu_events(), vec![MenuEvent::OpenPreferences]);
}

#[test]
fn the_search_field_takes_the_text_and_the_delay() {
    let ctx = Context::default();
    apply_theme(&ctx);
    let mut shell = Shell::new();
    draw(&ctx, &mut shell);
    let field = Id::new(SEARCH_ID);
    let pos = centre(&ctx, field);
    frame(&ctx, &mut shell, click(pos));
    assert!(ctx.memory(|memory| memory.has_focus(field)), "no focus");

    frame(&ctx, &mut shell, vec![Event::Text("a".to_owned())]);

    assert_eq!(shell.gallery_mut().search_mut().as_str(), "a");
    // The typed text is painted where the hint stood.
    let output = ctx.run_ui(input(), |ui| shell.show(ui));
    assert!(painted_text(&output).contains(&"a".to_owned()));
    output.drop_without_applying_deltas();
}

#[test]
fn typing_in_the_field_narrows_the_gallery_after_the_quiet_window() {
    let ctx = Context::default();
    apply_theme(&ctx);
    let mut shell = Shell::new();
    let folder = PathBuf::from("/images");
    shell.set_folder(Some(folder.clone()));
    shell.set_items(
        ["a.png", "b.png"]
            .into_iter()
            .map(|name| GridItem::new(folder.join(name), 1, None))
            .collect(),
    );
    draw(&ctx, &mut shell);
    let pos = centre(&ctx, Id::new(SEARCH_ID));
    frame(&ctx, &mut shell, click(pos));

    frame(&ctx, &mut shell, vec![Event::Text("b".to_owned())]);
    assert_eq!(shell.gallery_mut().visible_len(), 2, "the filter waits");

    std::thread::sleep(SEARCH_DEBOUNCE + Duration::from_millis(20));
    frame(&ctx, &mut shell, vec![]);

    assert_eq!(
        shell.gallery_mut().visible_len(),
        1,
        "the field armed the search and the timer applied it"
    );
}

#[test]
fn the_menu_buttons_keep_the_gap_of_the_design() {
    let ctx = Context::default();
    apply_theme(&ctx);
    let mut shell = Shell::new();
    draw(&ctx, &mut shell);
    let output = ctx.run_ui(input(), |ui| shell.show(ui));
    let rect = |label: &str| {
        output
            .shapes
            .iter()
            .find_map(|clipped| match &clipped.shape {
                egui::epaint::Shape::Text(shape) if shape.galley.job.text == label => {
                    Some(shape.visual_bounding_rect())
                }
                _ => None,
            })
            .unwrap_or_else(|| panic!("{label} is not painted"))
    };
    let (file, view, settings) = (rect("File"), rect("View"), rect("Settings"));
    output.drop_without_applying_deltas();
    // Two paddings and the gap sit between two neighbouring labels.
    let want = 2.0 * MENU_PAD.x + MENU_GAP;
    assert!(
        (view.min.x - file.max.x - want).abs() < 1.0,
        "File and View are {} apart",
        view.min.x - file.max.x
    );
    assert!(
        (settings.min.x - view.max.x - want).abs() < 1.0,
        "View and Settings are {} apart",
        settings.min.x - view.max.x
    );
}

#[test]
fn a_narrow_window_leaves_out_the_field_without_panicking() {
    let ctx = Context::default();
    apply_theme(&ctx);
    let mut shell = Shell::new();
    let mut texts = Vec::new();
    for _ in 0..2 {
        let mut narrow = input();
        narrow.screen_rect = Some(egui::Rect::from_min_size(
            Pos2::ZERO,
            egui::vec2(360.0, 400.0),
        ));
        let output = ctx.run_ui(narrow, |ui| shell.show(ui));
        texts = painted_text(&output);
        output.drop_without_applying_deltas();
    }
    assert!(
        !texts.contains(&SEARCH_HINT.to_owned()),
        "the field is left out: {texts:?}"
    );
    assert!(texts.contains(&"Tarragon".to_owned()), "{texts:?}");
    // The icon buttons go too, so nothing overlaps the menus.
    for label in ["Show log", "Settings"] {
        assert!(
            ctx.read_response(Id::new(("tarragon-icon-button", label)))
                .is_none(),
            "{label} still draws in a narrow window"
        );
    }
}

#[test]
fn a_focus_request_in_a_narrow_window_is_dropped() {
    let ctx = Context::default();
    apply_theme(&ctx);
    let mut shell = Shell::new();
    let mut narrow = input();
    narrow.screen_rect = Some(egui::Rect::from_min_size(
        Pos2::ZERO,
        egui::vec2(360.0, 400.0),
    ));
    shell.gallery.focus_search();
    ctx.run_ui(narrow, |ui| shell.show(ui))
        .drop_without_applying_deltas();
    frame(&ctx, &mut shell, Vec::new());
    frame(&ctx, &mut shell, Vec::new());
    assert!(
        !ctx.memory(|m| m.has_focus(Id::new(SEARCH_ID))),
        "the field took focus from a request made while it was hidden"
    );
}

#[test]
fn the_bar_keeps_the_gaps_of_the_design() {
    let ctx = Context::default();
    apply_theme(&ctx);
    let mut shell = Shell::new();
    draw(&ctx, &mut shell);
    draw(&ctx, &mut shell);
    // The menu scope keeps the bar's own spacing, so the gap between the two
    // icon buttons is the extra-small step and not two points more.
    let log = centre(&ctx, Id::new(("tarragon-icon-button", "Show log")));
    let settings = centre(&ctx, Id::new(("tarragon-icon-button", "Settings")));
    let gap = settings.x - log.x - widgets::ICON_BUTTON_SIZE;
    assert!(
        (gap - tokens::SPACING_XS).abs() < 1.0,
        "the icon buttons are {gap} apart"
    );
}

#[test]
fn the_bar_sits_on_the_panel_fill_with_one_row() {
    let ctx = Context::default();
    apply_theme(&ctx);
    let mut shell = Shell::new();
    draw(&ctx, &mut shell);
    let output = ctx.run_ui(input(), |ui| shell.show(ui));
    let bar = output
        .shapes
        .iter()
        .find_map(|clipped| match &clipped.shape {
            egui::epaint::Shape::Rect(rect)
                if rect.fill == tokens::BG_SECONDARY && rect.rect.width() > 1000.0 =>
            {
                Some(rect.rect)
            }
            _ => None,
        });
    output.drop_without_applying_deltas();
    let bar = bar.expect("the bar fill is not painted");
    // The bar is the field plus its padding, and the panel keeps a point of its
    // own at the edge for the separator.
    let want = SEARCH_HEIGHT + 2.0 * BAR_PAD_Y;
    assert!(
        (bar.height() - want).abs() < 1.5,
        "the bar is {} tall, want {want}",
        bar.height()
    );
    // The logo, the wordmark, the menus, the field, and the buttons share it.
    assert!(bar.width() > 1300.0, "the bar is {} wide", bar.width());
    assert_eq!(bar.min, Pos2::ZERO);
}
