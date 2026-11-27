//! Tests for the Preferences window.

use super::draft::{CANVAS, CAP, Draft, MULTI, NEUTRAL, PALETTE, SHARE, WORKERS};
use super::{Group, Prefs, PrefsEvent, PrefsValues};
use crate::{test_support, tokens};
use egui::epaint::Shape;
use egui::{Color32, Context, Event, Id, Key, Modifiers, Pos2, Rect, vec2};

mod groups;

fn values() -> PrefsValues {
    PrefsValues {
        max_psd_workers: 3,
        max_multi_preview: 9,
        large_canvas_mp: 20.0,
        tile_grid: "3x3".to_owned(),
        color_tag_enabled: true,
        palette_size: 8,
        min_share: 0.10,
        neutral_s: 0.15,
        cache_dir: None,
        cache_format: "jpeg".to_owned(),
        clear_full_res: true,
        cache_cap_gb: 20,
        debug: false,
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

/// Every painted text with its rect and colour.
type Texts = Vec<(String, Rect, Color32)>;

fn collect(shape: &Shape, found: &mut Texts) {
    match shape {
        Shape::Vec(shapes) => shapes.iter().for_each(|s| collect(s, found)),
        Shape::Text(t) => {
            let rect = t.galley.rect.translate(t.pos.to_vec2());
            let color = t.galley.job.sections[0].format.color;
            found.push((t.galley.job.text.clone(), rect, color));
        }
        _ => {}
    }
}

fn frame(ctx: &Context, prefs: &mut Prefs, events: Vec<Event>) -> Texts {
    // The window is taller than the default test screen.
    frame_on(ctx, prefs, events, 1200.0)
}

/// One frame on a screen 800 points wide and `height` points tall.
fn frame_on(ctx: &Context, prefs: &mut Prefs, events: Vec<Event>, height: f32) -> Texts {
    let mut input = test_support::input(800.0, events);
    input.screen_rect = Some(Rect::from_min_size(Pos2::ZERO, vec2(800.0, height)));
    let output = ctx.run_ui(input, |ui| {
        prefs.show(ui.ctx());
    });
    let mut found = Vec::new();
    for clipped in &output.shapes {
        collect(&clipped.shape, &mut found);
    }
    output.drop_without_applying_deltas();
    found
}

/// An open window, drawn for two frames: an egui window is hidden in its first.
fn open_prefs() -> (Context, Prefs) {
    let ctx = test_support::context();
    let mut prefs = Prefs::default();
    prefs.open(values(), "1.4 GB".to_owned());
    for _ in 0..6 {
        frame(&ctx, &mut prefs, vec![]);
    }
    (ctx, prefs)
}

fn type_into(prefs: &mut Prefs, index: usize, text: &str) {
    let draft = prefs.draft.as_mut().expect("the window is open");
    draft.numbers[index] = text.to_owned();
}

fn click_text(ctx: &Context, prefs: &mut Prefs, want: &str) {
    let texts = frame(ctx, prefs, vec![]);
    let (_, rect, _) = texts
        .iter()
        .find(|(text, _, _)| text == want)
        .unwrap_or_else(|| panic!("no text {want:?} in {texts:?}"));
    let at: Pos2 = rect.center();
    frame(ctx, prefs, test_support::click(at));
}

/// An open window that shows `group`.
fn open_on(group: Group) -> (Context, Prefs) {
    let (ctx, mut prefs) = open_prefs();
    prefs.show_group(group);
    frame(&ctx, &mut prefs, vec![]);
    (ctx, prefs)
}

fn has(texts: &Texts, want: &str) -> bool {
    texts.iter().any(|(t, _, _)| t == want)
}

fn rect_of(texts: &Texts, want: &str) -> Rect {
    texts
        .iter()
        .find_map(|(t, rect, _)| (t == want).then_some(*rect))
        .unwrap_or_else(|| panic!("no text {want:?} in {texts:?}"))
}

fn colour_of(texts: &Texts, want: &str) -> Color32 {
    texts
        .iter()
        .find_map(|(t, _, c)| (t == want).then_some(*c))
        .unwrap_or_else(|| panic!("no text {want:?} in {texts:?}"))
}

#[test]
fn the_typed_text_of_every_number_parses_back_to_the_values() {
    assert_eq!(Draft::new(values()).parse(), Some(values()));
}

#[test]
fn a_number_that_does_not_parse_makes_the_whole_parse_fail() {
    for index in [WORKERS, MULTI, CANVAS, PALETTE, SHARE, NEUTRAL, CAP] {
        let mut draft = Draft::new(values());
        draft.numbers[index] = "abc".to_owned();
        assert!(draft.parse().is_none(), "number {index}");
        assert!(!draft.valid(index), "number {index}");
    }
}

#[test]
fn a_whole_number_field_rejects_a_sign_and_a_fraction() {
    for text in ["-1", "2.5", ""] {
        let mut draft = Draft::new(values());
        draft.numbers[WORKERS] = text.to_owned();
        assert!(draft.parse().is_none(), "{text:?}");
    }
}

#[test]
fn a_float_field_rejects_infinity_and_nan_and_accepts_a_sign() {
    for text in ["inf", "NaN"] {
        let mut draft = Draft::new(values());
        draft.numbers[CANVAS] = text.to_owned();
        assert!(draft.parse().is_none(), "{text:?}");
    }
    let mut draft = Draft::new(values());
    draft.numbers[CANVAS] = "-1.5".to_owned();
    assert_eq!(draft.parse().map(|v| v.large_canvas_mp), Some(-1.5));
}

#[test]
fn spaces_around_a_number_are_ignored() {
    let mut draft = Draft::new(values());
    draft.numbers[WORKERS] = "  4 ".to_owned();
    assert_eq!(draft.parse().map(|v| v.max_psd_workers), Some(4));
}

#[test]
fn a_bad_number_disables_save_and_a_good_one_enables_it() {
    let (ctx, mut prefs) = open_prefs();
    type_into(&mut prefs, WORKERS, "abc");
    frame(&ctx, &mut prefs, vec![]);
    click_text(&ctx, &mut prefs, "Save");
    assert_eq!(prefs.take_events().len(), 0);
    assert!(prefs.is_open());

    type_into(&mut prefs, WORKERS, "4");
    frame(&ctx, &mut prefs, vec![]);
    click_text(&ctx, &mut prefs, "Save");
    let events = prefs.take_events();
    let expected = PrefsValues {
        max_psd_workers: 4,
        ..values()
    };
    assert_eq!(events, vec![PrefsEvent::Ok(expected)]);
    assert!(!prefs.is_open());
}

#[test]
fn a_bad_number_turns_its_label_and_its_group_coral() {
    let (ctx, mut prefs) = open_prefs();
    type_into(&mut prefs, CAP, "abc");
    let texts = frame(&ctx, &mut prefs, vec![]);
    // The Cache group is not shown, so only its list row tells.
    assert_eq!(colour_of(&texts, "Cache"), tokens::CORAL_STRONG);
    assert_eq!(colour_of(&texts, "Debug"), tokens::TEXT_PRIMARY);
    prefs.show_group(Group::Cache);
    let texts = frame(&ctx, &mut prefs, vec![]);
    assert_eq!(
        colour_of(&texts, "Cache size cap (GiB, 0 = off)"),
        tokens::CORAL_STRONG
    );
    assert_eq!(colour_of(&texts, "Cache format"), tokens::TEXT_TERTIARY);
}

#[test]
fn cancel_closes_the_window_and_reopening_shows_the_old_values() {
    let (ctx, mut prefs) = open_prefs();
    type_into(&mut prefs, WORKERS, "7");
    frame(&ctx, &mut prefs, vec![]);
    click_text(&ctx, &mut prefs, "Cancel");
    assert_eq!(prefs.take_events(), vec![PrefsEvent::Cancel]);
    assert!(!prefs.is_open());

    prefs.open(values(), "1.4 GB".to_owned());
    let draft = prefs.draft.as_ref().expect("the window is open");
    assert_eq!(draft.numbers[WORKERS], "3");
}

#[test]
fn escape_cancels_and_drops_the_typed_text() {
    let (ctx, mut prefs) = open_prefs();
    type_into(&mut prefs, WORKERS, "7");
    frame(&ctx, &mut prefs, vec![escape()]);
    assert_eq!(prefs.take_events(), vec![PrefsEvent::Cancel]);
    assert!(!prefs.is_open());
    assert!(prefs.draft.is_none());
}

/// The centre of the close button: the cross at the right of the title row.
fn close_spot(texts: &Texts) -> Pos2 {
    let title = rect_of(texts, "Preferences");
    let save = rect_of(texts, "Save");
    // The button is 32 points wide, and its right edge lines up with the
    // right edge of Save.
    Pos2::new(save.right() - 16.0, title.center().y)
}

#[test]
fn the_close_button_cancels() {
    let (ctx, mut prefs) = open_prefs();
    let texts = frame(&ctx, &mut prefs, vec![]);
    let at = close_spot(&texts);
    frame(&ctx, &mut prefs, test_support::click(at));
    assert_eq!(prefs.take_events(), vec![PrefsEvent::Cancel]);
}

#[test]
fn the_picked_cache_folder_comes_back_with_save() {
    let (ctx, mut prefs) = open_prefs();
    prefs.set_cache_dir("/data/cache".to_owned());
    frame(&ctx, &mut prefs, vec![]);
    click_text(&ctx, &mut prefs, "Save");
    let events = prefs.take_events();
    let Some(PrefsEvent::Ok(got)) = events.first() else {
        panic!("no Ok event: {events:?}");
    };
    assert_eq!(got.cache_dir.as_deref(), Some("/data/cache"));
}

#[test]
fn a_closed_window_draws_nothing_and_ignores_the_picked_folder() {
    let ctx = test_support::context();
    let mut prefs = Prefs::default();
    prefs.set_cache_dir("/data/cache".to_owned());
    assert!(!prefs.is_open());
    assert_eq!(frame(&ctx, &mut prefs, vec![escape()]).len(), 0);
    assert_eq!(prefs.take_events().len(), 0);
}

#[test]
fn browse_queues_an_event_and_keeps_the_window_open() {
    let (ctx, mut prefs) = open_on(Group::Cache);
    click_text(&ctx, &mut prefs, "Browse...");
    assert_eq!(prefs.take_events(), vec![PrefsEvent::BrowseCacheDir]);
    assert!(prefs.is_open());
}

#[test]
fn purge_queues_an_event_and_keeps_the_window_open() {
    let (ctx, mut prefs) = open_on(Group::Cache);
    click_text(&ctx, &mut prefs, "Purge cache");
    assert_eq!(prefs.take_events(), vec![PrefsEvent::PurgeCache]);
    assert!(prefs.is_open());
}

#[test]
fn while_purging_the_strip_shows_and_save_cancel_purge_close_and_escape_do_nothing() {
    let (ctx, mut prefs) = open_on(Group::Cache);
    prefs.set_purging(true);
    let texts = frame(&ctx, &mut prefs, vec![]);
    assert!(has(&texts, "Purging cache..."));
    for button in ["Save", "Cancel", "Purge cache"] {
        click_text(&ctx, &mut prefs, button);
    }
    let at = close_spot(&frame(&ctx, &mut prefs, vec![]));
    frame(&ctx, &mut prefs, test_support::click(at));
    frame(&ctx, &mut prefs, vec![escape()]);
    assert_eq!(prefs.take_events().len(), 0);
    assert!(prefs.is_open());
}

#[test]
fn while_purging_browse_still_works_and_the_buttons_work_again_after() {
    let (ctx, mut prefs) = open_on(Group::Cache);
    prefs.set_purging(true);
    click_text(&ctx, &mut prefs, "Browse...");
    assert_eq!(prefs.take_events(), vec![PrefsEvent::BrowseCacheDir]);

    prefs.set_purging(false);
    let texts = frame(&ctx, &mut prefs, vec![]);
    assert!(!has(&texts, "Purging cache..."));
    click_text(&ctx, &mut prefs, "Cancel");
    assert_eq!(prefs.take_events(), vec![PrefsEvent::Cancel]);
}

#[test]
fn an_error_shows_in_the_footer_and_opening_clears_it() {
    let (ctx, mut prefs) = open_prefs();
    prefs.set_error(Some("could not save".to_owned()));
    let texts = frame(&ctx, &mut prefs, vec![]);
    let error = rect_of(&texts, "could not save");
    let cancel = rect_of(&texts, "Cancel");
    assert!(error.right() < cancel.left(), "{error:?} {cancel:?}");
    assert!((error.center().y - cancel.center().y).abs() < 2.0);

    prefs.set_error(None);
    assert!(!has(&frame(&ctx, &mut prefs, vec![]), "could not save"));

    prefs.set_error(Some("old".to_owned()));
    prefs.open(values(), String::new());
    assert!(!has(&frame(&ctx, &mut prefs, vec![]), "old"));
}

#[test]
fn save_sits_right_of_cancel() {
    let (ctx, mut prefs) = open_prefs();
    let texts = frame(&ctx, &mut prefs, vec![]);
    assert!(rect_of(&texts, "Save").center().x > rect_of(&texts, "Cancel").center().x);
}

#[test]
fn on_a_short_screen_the_buttons_stay_on_screen_and_save_still_works() {
    let ctx = test_support::context();
    let mut prefs = Prefs::default();
    prefs.open(values(), "1.4 GB".to_owned());
    frame_on(&ctx, &mut prefs, vec![], 450.0);
    let texts = frame_on(&ctx, &mut prefs, vec![], 450.0);
    for button in ["Save", "Cancel"] {
        let rect = rect_of(&texts, button);
        assert!(
            rect.top() >= 0.0 && rect.bottom() <= 450.0,
            "{button}: {rect:?}"
        );
    }
    let at = rect_of(&texts, "Save").center();
    frame_on(&ctx, &mut prefs, test_support::click(at), 450.0);
    assert_eq!(prefs.take_events(), vec![PrefsEvent::Ok(values())]);
}

/// The rect of the whole window.
fn window_rect(ctx: &Context) -> Rect {
    // egui hashes the title as an optional string to make the window id.
    ctx.memory(|m| m.area_rect(Id::new(Some("Preferences"))))
        .expect("the window is drawn")
}

#[test]
fn the_window_takes_the_page_size_held_inside_the_screen() {
    let (ctx, mut prefs) = open_prefs();
    frame(&ctx, &mut prefs, vec![]);
    let rect = window_rect(&ctx);
    // The 800 point test screen holds 768 of the page's 800 points.
    assert!((rect.width() - 768.0).abs() < 0.5, "{rect:?}");
    assert!((rect.height() - 560.0).abs() < 0.5, "{rect:?}");
    frame_on(&ctx, &mut prefs, vec![], 450.0);
    frame_on(&ctx, &mut prefs, vec![], 450.0);
    let short = window_rect(&ctx);
    assert!((short.height() - 418.0).abs() < 0.5, "{short:?}");
}

#[test]
fn the_window_keeps_its_size_while_the_purge_runs() {
    let (ctx, mut prefs) = open_on(Group::Cache);
    let before = window_rect(&ctx);
    prefs.set_purging(true);
    for _ in 0..6 {
        frame(&ctx, &mut prefs, vec![]);
    }
    assert_eq!(window_rect(&ctx), before);
}
