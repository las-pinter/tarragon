//! Tests for the group list and the inputs of each group.

use super::super::fields::STEP;
use super::super::groups::{CACHE_FORMATS, DEFAULT_CACHE_DIR, TILE_GRIDS, description};
use super::*;

#[test]
fn the_list_names_the_four_groups_and_the_first_is_shown() {
    let (ctx, mut prefs) = open_prefs();
    let texts = frame(&ctx, &mut prefs, vec![]);
    for group in Group::ALL {
        assert!(has(&texts, group.title()), "{}", group.title());
    }
    assert_eq!(prefs.group(), Group::Performance);
    assert_eq!(colour_of(&texts, "Performance"), tokens::CORAL_STRONG);
    assert!(has(&texts, "Max PSD workers"));
    assert!(!has(&texts, "Palette size"), "one group shows at a time");
}

#[test]
fn each_group_shows_its_rows_so_every_setting_is_reachable() {
    let rows: [(Group, &[&str]); 4] = [
        (
            Group::Performance,
            &[
                "Max PSD workers",
                "Max multi-preview",
                "Large canvas threshold (MP)",
                "Tile grid size",
            ],
        ),
        (
            Group::ColorTagging,
            &[
                "Enable color tagging",
                "Palette size",
                "Minimum share",
                "Neutral saturation threshold",
            ],
        ),
        (
            Group::Cache,
            &[
                "Cache folder",
                "Cache format",
                "Cache size cap (GiB, 0 = off)",
                "Clear full-size cache on exit",
                "Cache size",
            ],
        ),
        (Group::Debug, &["Enable debug logging"]),
    ];
    for (group, labels) in rows {
        let (ctx, mut prefs) = open_on(group);
        let texts = frame(&ctx, &mut prefs, vec![]);
        for label in labels {
            assert!(has(&texts, label), "{label} in {}", group.title());
        }
        assert!(has(&texts, description(group)), "{}", group.title());
    }
}

#[test]
fn a_click_on_a_list_row_shows_that_group_and_it_lasts_after_a_reopen() {
    let (ctx, mut prefs) = open_prefs();
    click_text(&ctx, &mut prefs, "Debug");
    let texts = frame(&ctx, &mut prefs, vec![]);
    assert_eq!(prefs.group(), Group::Debug);
    assert!(has(&texts, "Enable debug logging"));
    click_text(&ctx, &mut prefs, "Cancel");
    prefs.open(values(), "1.4 GB".to_owned());
    assert_eq!(prefs.group(), Group::Debug);
}

#[test]
fn the_group_lines_name_when_changes_apply_and_the_unread_settings() {
    assert!(description(Group::ColorTagging).contains("next folder open"));
    let cache = description(Group::Cache);
    assert!(cache.contains("cache folder and the size cap apply at the next start"));
    assert!(cache.contains("nothing reads it yet"));
    assert!(description(Group::Performance).contains("nothing reads them yet"));
}

#[test]
fn the_choices_are_the_registry_choices() {
    assert_eq!(TILE_GRIDS, tarragon_db::settings::TILE_GRID_SIZES);
    assert_eq!(CACHE_FORMATS, tarragon_db::settings::CACHE_FORMATS);
    assert_eq!(TILE_GRIDS, ["1x1", "2x2", "3x3", "4x4"]);
}

#[test]
fn the_choice_fields_show_the_current_choice() {
    let (ctx, mut prefs) = open_prefs();
    assert!(has(&frame(&ctx, &mut prefs, vec![]), "3x3"));
    prefs.show_group(Group::Cache);
    assert!(has(&frame(&ctx, &mut prefs, vec![]), "jpeg"));
}

#[test]
fn a_picked_cache_folder_replaces_the_default_text() {
    let (ctx, mut prefs) = open_on(Group::Cache);
    assert!(has(&frame(&ctx, &mut prefs, vec![]), DEFAULT_CACHE_DIR));
    prefs.set_cache_dir("/data/cache".to_owned());
    let texts = frame(&ctx, &mut prefs, vec![]);
    assert!(has(&texts, "/data/cache"));
    assert!(!has(&texts, DEFAULT_CACHE_DIR));
}

#[test]
fn the_cache_size_sits_right_of_its_label_and_can_change() {
    let (ctx, mut prefs) = open_on(Group::Cache);
    let texts = frame(&ctx, &mut prefs, vec![]);
    let label = rect_of(&texts, "Cache size");
    let value = rect_of(&texts, "1.4 GB");
    assert!(value.left() >= label.right() + 4.0, "{label:?} {value:?}");
    prefs.set_cache_size("0 B".to_owned());
    let texts = frame(&ctx, &mut prefs, vec![]);
    assert!(has(&texts, "0 B"));
    assert!(!has(&texts, "1.4 GB"));
}

#[test]
fn a_click_on_a_toggle_label_flips_the_setting() {
    let (ctx, mut prefs) = open_on(Group::Debug);
    click_text(&ctx, &mut prefs, "Enable debug logging");
    click_text(&ctx, &mut prefs, "Save");
    let events = prefs.take_events();
    let Some(PrefsEvent::Ok(got)) = events.first() else {
        panic!("no Ok event: {events:?}");
    };
    assert!(got.debug);
}

/// The slider rail: the row under the threshold label, as wide as the page.
fn slider_spot(texts: &Texts, share: f32) -> Pos2 {
    let label = rect_of(texts, "Neutral saturation threshold");
    let value = rect_of(texts, "0.15");
    // The knob centre runs from 8 points inside the left edge to 8 inside
    // the right edge, 10 below the label row plus half the 20 point slider.
    let left = label.left() + 8.0;
    let right = value.right() - 8.0;
    Pos2::new(left + share * (right - left), label.bottom() + 20.0)
}

#[test]
fn a_click_on_the_slider_sets_the_threshold_and_shows_it() {
    let (ctx, mut prefs) = open_on(Group::ColorTagging);
    let texts = frame(&ctx, &mut prefs, vec![]);
    let at = slider_spot(&texts, 0.5);
    frame(&ctx, &mut prefs, test_support::click(at));
    let draft = prefs.draft.as_ref().expect("open");
    assert_eq!(draft.numbers[NEUTRAL], "0.50");
    assert!(has(&frame(&ctx, &mut prefs, vec![]), "0.50"));
}

#[test]
fn the_arrow_keys_step_a_focused_slider() {
    let (ctx, mut prefs) = open_on(Group::ColorTagging);
    let texts = frame(&ctx, &mut prefs, vec![]);
    // A click focuses it and sets 0.15 again.
    let at = slider_spot(&texts, 0.15);
    frame(&ctx, &mut prefs, test_support::click(at));
    let key = |key| Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::NONE,
    };
    let start: f32 = prefs.draft.as_ref().expect("open").numbers[NEUTRAL]
        .parse()
        .expect("a number");
    frame(
        &ctx,
        &mut prefs,
        vec![key(Key::ArrowRight), key(Key::ArrowRight)],
    );
    let now: f32 = prefs.draft.as_ref().expect("open").numbers[NEUTRAL]
        .parse()
        .expect("a number");
    assert!((now - start - 2.0 * STEP).abs() < 0.001, "{start} -> {now}");
}
