//! Tests for the Preferences window.

use super::draft::{CANVAS, CAP, Draft, MULTI, NEUTRAL, PALETTE, SHARE, WORKERS};
use super::{Prefs, PrefsEvent, PrefsValues};
use crate::{test_support, tokens};
use egui::epaint::Shape;
use egui::{Color32, Context, Event, Key, Modifiers, Pos2, Rect, vec2};

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
fn a_frame_shows_a_row_for_each_number() {
    let (ctx, mut prefs) = open_prefs();
    let texts = frame(&ctx, &mut prefs, vec![]);
    for label in ["Max PSD Workers", "Cache size cap (GiB, 0 = off)", "OK"] {
        assert!(texts.iter().any(|(t, _, _)| t == label), "{label}");
    }
}

#[test]
fn a_bad_number_disables_ok_and_a_good_one_enables_it() {
    let (ctx, mut prefs) = open_prefs();
    type_into(&mut prefs, WORKERS, "abc");
    frame(&ctx, &mut prefs, vec![]);
    click_text(&ctx, &mut prefs, "OK");
    assert_eq!(prefs.take_events().len(), 0);
    assert!(prefs.is_open());

    type_into(&mut prefs, WORKERS, "4");
    frame(&ctx, &mut prefs, vec![]);
    click_text(&ctx, &mut prefs, "OK");
    let events = prefs.take_events();
    let expected = PrefsValues {
        max_psd_workers: 4,
        ..values()
    };
    assert_eq!(events, vec![PrefsEvent::Ok(expected)]);
    assert!(!prefs.is_open());
}

#[test]
fn a_bad_number_shows_its_label_in_coral() {
    let (ctx, mut prefs) = open_prefs();
    type_into(&mut prefs, WORKERS, "abc");
    let texts = frame(&ctx, &mut prefs, vec![]);
    let colour_of = |want: &str| texts.iter().find(|(t, _, _)| t == want).map(|(_, _, c)| *c);
    assert_eq!(colour_of("Max PSD Workers"), Some(tokens::CORAL_STRONG));
    assert_eq!(colour_of("Max Multi-Preview"), Some(tokens::TEXT_PRIMARY));
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

#[test]
fn the_picked_cache_folder_comes_back_with_ok() {
    let (ctx, mut prefs) = open_prefs();
    prefs.set_cache_dir("/data/cache".to_owned());
    frame(&ctx, &mut prefs, vec![]);
    click_text(&ctx, &mut prefs, "OK");
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

fn has(texts: &Texts, want: &str) -> bool {
    texts.iter().any(|(t, _, _)| t == want)
}

#[test]
fn a_frame_shows_the_four_groups_and_the_thirteen_value_rows() {
    let (ctx, mut prefs) = open_prefs();
    let texts = frame(&ctx, &mut prefs, vec![]);
    for group in ["Performance", "Color Tagging", "Cache", "Debug"] {
        assert!(has(&texts, group), "group {group}");
    }
    let rows = [
        "Max PSD Workers",
        "Max Multi-Preview",
        "Large Canvas Threshold (MP)",
        "Tile Grid Size",
        "Enable Color Tagging",
        "Color Tag Palette Size",
        "Color Tag Minimum Share",
        "Neutral Saturation Threshold",
        "Cache Directory",
        "Cache Format",
        "Clear Full-Res Cache on Exit",
        "Cache size cap (GiB, 0 = off)",
        "Enable Debug Logging",
    ];
    for row in rows {
        assert!(has(&texts, row), "row {row}");
    }
    assert!(has(&texts, "Cache Size"));
    assert!(has(&texts, "1.4 GB"));
    assert!(has(&texts, "Default cache directory"));
    assert!(!has(&texts, "Purging cache..."));
}

#[test]
fn the_footnote_names_when_changes_apply_and_the_unread_settings() {
    let (ctx, mut prefs) = open_prefs();
    let texts = frame(&ctx, &mut prefs, vec![]);
    let note = texts
        .iter()
        .find(|(t, _, _)| t.starts_with("Colour tagging changes"))
        .expect("the footnote");
    assert!(note.0.contains("next folder open"), "{}", note.0);
    assert!(note.0.contains("cache folder applies at the next start"));
    assert!(note.0.contains("nothing reads them yet"));
}

#[test]
fn the_choices_are_the_registry_choices() {
    assert_eq!(
        super::form::TILE_GRIDS,
        tarragon_db::settings::TILE_GRID_SIZES
    );
    assert_eq!(
        super::form::CACHE_FORMATS,
        tarragon_db::settings::CACHE_FORMATS
    );
    assert_eq!(super::form::TILE_GRIDS, ["1x1", "2x2", "3x3", "4x4"]);
}

#[test]
fn the_choice_rows_show_the_current_choice() {
    let (ctx, mut prefs) = open_prefs();
    let texts = frame(&ctx, &mut prefs, vec![]);
    assert!(has(&texts, "3x3"));
    assert!(has(&texts, "jpeg"));
}

#[test]
fn a_picked_cache_folder_replaces_the_default_text() {
    let (ctx, mut prefs) = open_prefs();
    prefs.set_cache_dir("/data/cache".to_owned());
    let texts = frame(&ctx, &mut prefs, vec![]);
    assert!(has(&texts, "/data/cache"));
    assert!(!has(&texts, "Default cache directory"));
}

#[test]
fn browse_queues_an_event_and_keeps_the_window_open() {
    let (ctx, mut prefs) = open_prefs();
    click_text(&ctx, &mut prefs, "Browse...");
    assert_eq!(prefs.take_events(), vec![PrefsEvent::BrowseCacheDir]);
    assert!(prefs.is_open());
}

#[test]
fn purge_queues_an_event_and_keeps_the_window_open() {
    let (ctx, mut prefs) = open_prefs();
    click_text(&ctx, &mut prefs, "Purge Cache");
    assert_eq!(prefs.take_events(), vec![PrefsEvent::PurgeCache]);
    assert!(prefs.is_open());
}

#[test]
fn while_purging_the_strip_shows_and_ok_cancel_purge_and_escape_do_nothing() {
    let (ctx, mut prefs) = open_prefs();
    prefs.set_purging(true);
    let texts = frame(&ctx, &mut prefs, vec![]);
    assert!(has(&texts, "Purging cache..."));
    for button in ["OK", "Cancel", "Purge Cache"] {
        click_text(&ctx, &mut prefs, button);
    }
    frame(&ctx, &mut prefs, vec![escape()]);
    assert_eq!(prefs.take_events().len(), 0);
    assert!(prefs.is_open());
}

#[test]
fn while_purging_browse_still_works_and_the_buttons_work_again_after() {
    let (ctx, mut prefs) = open_prefs();
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
fn the_cache_size_text_can_change_while_the_window_is_open() {
    let (ctx, mut prefs) = open_prefs();
    prefs.set_cache_size("0 B".to_owned());
    let texts = frame(&ctx, &mut prefs, vec![]);
    assert!(has(&texts, "0 B"));
    assert!(!has(&texts, "1.4 GB"));
}

#[test]
fn an_error_shows_above_the_buttons_and_opening_clears_it() {
    let (ctx, mut prefs) = open_prefs();
    prefs.set_error(Some("could not save".to_owned()));
    let texts = frame(&ctx, &mut prefs, vec![]);
    assert!(has(&texts, "could not save"));
    let error_y = rect_of(&texts, "could not save").top();
    let ok_y = rect_of(&texts, "OK").top();
    assert!(error_y < ok_y, "{error_y:?} {ok_y:?}");

    prefs.set_error(None);
    assert!(!has(&frame(&ctx, &mut prefs, vec![]), "could not save"));

    prefs.set_error(Some("old".to_owned()));
    prefs.open(values(), String::new());
    assert!(!has(&frame(&ctx, &mut prefs, vec![]), "old"));
}

fn rect_of(texts: &Texts, want: &str) -> Rect {
    texts
        .iter()
        .find_map(|(t, rect, _)| (t == want).then_some(*rect))
        .unwrap_or_else(|| panic!("no text {want:?} in {texts:?}"))
}

#[test]
fn ok_sits_left_of_cancel() {
    let (ctx, mut prefs) = open_prefs();
    let texts = frame(&ctx, &mut prefs, vec![]);
    assert!(rect_of(&texts, "OK").center().x < rect_of(&texts, "Cancel").center().x);
}

#[test]
fn on_a_short_screen_the_buttons_stay_on_screen_and_ok_still_works() {
    let ctx = test_support::context();
    let mut prefs = Prefs::default();
    prefs.open(values(), "1.4 GB".to_owned());
    frame_on(&ctx, &mut prefs, vec![], 450.0);
    let texts = frame_on(&ctx, &mut prefs, vec![], 450.0);
    for button in ["OK", "Cancel"] {
        let rect = rect_of(&texts, button);
        assert!(
            rect.top() >= 0.0 && rect.bottom() <= 450.0,
            "{button}: {rect:?}"
        );
    }
    let at = rect_of(&texts, "OK").center();
    frame_on(&ctx, &mut prefs, test_support::click(at), 450.0);
    assert_eq!(prefs.take_events(), vec![PrefsEvent::Ok(values())]);
}

#[test]
fn on_a_tall_screen_the_dialog_is_not_cut_short() {
    let (ctx, mut prefs) = open_prefs();
    let texts = frame(&ctx, &mut prefs, vec![]);
    // The last group is in view without a scroll.
    assert!(rect_of(&texts, "Enable Debug Logging").bottom() < 1200.0);
    assert!(rect_of(&texts, "Debug").top() > rect_of(&texts, "Cache Size").top());
}
