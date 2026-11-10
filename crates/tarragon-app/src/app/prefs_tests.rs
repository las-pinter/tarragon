//! Tests that the app loads, saves, and applies the Preferences. Every frame
//! is headless and the database sits in a temp folder.

use eframe::egui::Id;
use tarragon_ui::prefs::PrefsValues;
use time::UtcOffset;
use tracing_subscriber::layer::SubscriberExt;

use super::preview_tests::Rig;
use super::*;
use crate::logging::ring::RingLayer;
use crate::prefs;

/// The defaults of the 13 settings.
fn defaults() -> PrefsValues {
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

/// Open the window with `values`, as the menu does, and draw it until it has
/// its size. The first frames paint no buttons.
fn open_with(rig: &mut Rig, values: PrefsValues) {
    rig.app.shell.prefs_mut().open(values, "unknown".to_owned());
    for _ in 0..4 {
        rig.frame(Vec::new());
    }
}

#[test]
fn load_on_a_fresh_database_gives_the_defaults() {
    let rig = Rig::new(&[], &[], 9);
    assert_eq!(prefs::load(&rig.app.db), defaults());
}

#[test]
fn a_blank_cache_folder_is_stored_as_unset_and_a_path_is_trimmed() {
    let with = |dir: Option<&str>| {
        let values = PrefsValues {
            cache_dir: dir.map(str::to_owned),
            ..defaults()
        };
        let pairs = prefs::pairs(&values);
        let (_, value) = pairs
            .into_iter()
            .find(|(key, _)| *key == SettingKey::CacheDir)
            .expect("the cache folder pair");
        value
    };
    assert_eq!(with(None), SettingValue::None);
    assert_eq!(with(Some("")), SettingValue::None);
    assert_eq!(with(Some(" \t ")), SettingValue::None);
    assert_eq!(with(Some("  /a/b ")), SettingValue::Text("/a/b".to_owned()));
}

#[test]
fn a_float_is_written_as_its_decimal_text() {
    let pairs = prefs::pairs(&defaults());
    let share = pairs
        .iter()
        .find(|(key, _)| *key == SettingKey::ColorTagMinShare)
        .expect("the share pair");
    assert_eq!(share.1, SettingValue::Float(0.10));
}

#[test]
fn ok_with_changed_values_stores_them() {
    let mut rig = Rig::new(&[], &[], 9);
    let changed = PrefsValues {
        max_psd_workers: 5,
        max_multi_preview: 4,
        large_canvas_mp: 30.5,
        tile_grid: "2x2".to_owned(),
        color_tag_enabled: false,
        palette_size: 6,
        min_share: 0.3,
        neutral_s: 0.1,
        cache_dir: Some("/somewhere".to_owned()),
        cache_format: "jpeg".to_owned(),
        clear_full_res: false,
        cache_cap_gb: 7,
        debug: true,
    };
    open_with(&mut rig, changed.clone());
    rig.click("OK", false);
    assert!(!rig.app.shell.prefs_mut().is_open());
    assert_eq!(prefs::load(&rig.app.db), changed);
    // The stored number is the short decimal, not the widened `f32`.
    let stored = Settings::new(&rig.app.db).get(SettingKey::ColorTagMinShare);
    assert_eq!(stored.expect("read"), SettingValue::Float(0.3));
}

#[test]
fn ok_with_colour_tags_off_removes_the_swatch_row_at_once() {
    let mut rig = Rig::new(&[], &[], 9);
    assert!(rig.swatches() > 0);
    open_with(
        &mut rig,
        PrefsValues {
            color_tag_enabled: false,
            ..defaults()
        },
    );
    rig.click("OK", false);
    assert_eq!(rig.swatches(), 0);
}

/// The Preview and the Gallery colour flags of the app.
fn colour_flags(rig: &mut Rig) -> (bool, bool) {
    (
        rig.app.shell.preview_mut().colors_enabled(),
        rig.app.shell.gallery_mut().colors_enabled(),
    )
}

#[test]
fn ok_sets_both_colour_flags_from_the_saved_setting() {
    let mut rig = Rig::new(&[], &[], 9);
    assert_eq!(colour_flags(&mut rig), (true, true));
    let off = PrefsValues {
        color_tag_enabled: false,
        ..defaults()
    };
    open_with(&mut rig, off);
    rig.click("OK", false);
    assert_eq!(colour_flags(&mut rig), (false, false));
    open_with(&mut rig, defaults());
    rig.click("OK", false);
    assert_eq!(colour_flags(&mut rig), (true, true));
}

#[test]
fn a_failed_save_leaves_both_colour_flags_unchanged() {
    let mut rig = Rig::new(&[], &[], 9);
    let bad = PrefsValues {
        tile_grid: "9x9".to_owned(),
        color_tag_enabled: false,
        ..defaults()
    };
    open_with(&mut rig, bad);
    rig.click("OK", false);
    assert!(rig.app.shell.prefs_mut().is_open());
    assert_eq!(colour_flags(&mut rig), (true, true));
}

#[test]
fn cancel_stores_nothing() {
    let mut rig = Rig::new(&[], &[], 9);
    let changed = PrefsValues {
        cache_cap_gb: 7,
        debug: true,
        ..defaults()
    };
    open_with(&mut rig, changed);
    rig.click("Cancel", false);
    assert!(!rig.app.shell.prefs_mut().is_open());
    assert_eq!(prefs::load(&rig.app.db), defaults());
    assert!(!rig.app.debug);
}

#[test]
fn a_bad_tile_grid_keeps_the_window_open_with_the_error_and_stores_nothing() {
    let mut rig = Rig::new(&[], &[], 9);
    let bad = PrefsValues {
        tile_grid: "9x9".to_owned(),
        cache_cap_gb: 7,
        ..defaults()
    };
    open_with(&mut rig, bad);
    rig.click("OK", false);
    assert!(rig.app.shell.prefs_mut().is_open());
    let (texts, _) = rig.frame(Vec::new());
    assert!(
        texts.iter().any(|t| t.contains("tile_grid_size")),
        "{texts:?}"
    );
    assert_eq!(prefs::load(&rig.app.db), defaults());
}

#[test]
fn ok_with_debug_switches_the_log_level_and_the_log_checkbox_state() {
    let mut rig = Rig::new(&[], &[], 9);
    let (filter, control) = LogControl::new_filter();
    rig.app.control = control;
    let lines_ring = LogRing::new();
    let subscriber = tracing_subscriber::registry()
        .with(filter)
        .with(RingLayer::new(lines_ring.clone(), UtcOffset::UTC));
    tracing::subscriber::with_default(subscriber, || {
        tracing::debug!("before");
        open_with(
            &mut rig,
            PrefsValues {
                debug: true,
                ..defaults()
            },
        );
        rig.click("OK", false);
        tracing::debug!("after");
    });
    let lines: Vec<String> = lines_ring.snapshot().into_iter().map(|e| e.text).collect();
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(lines[0].ends_with("after"), "{lines:?}");
    // The Log panel draws its checkbox from this flag.
    assert!(rig.app.debug);
}

#[test]
fn ok_with_a_smaller_multi_preview_shows_fewer_pictures_at_once() {
    let names = ["a.png", "b.png", "c.png"];
    let mut rig = Rig::new(&names, &names, 9);
    rig.click("a.png", false);
    rig.click("b.png", true);
    rig.click("c.png", true);
    rig.pictures_reach(3);
    open_with(
        &mut rig,
        PrefsValues {
            max_multi_preview: 2,
            ..defaults()
        },
    );
    rig.click("OK", false);
    assert_eq!(rig.app.max_multi_preview, 2);
    rig.pictures_reach(2);
}

#[test]
fn ok_with_a_larger_multi_preview_loads_the_missing_pictures() {
    let names = ["a.png", "b.png", "c.png"];
    let mut rig = Rig::new(&names, &names, 2);
    rig.click("a.png", false);
    rig.click("b.png", true);
    rig.click("c.png", true);
    rig.pictures_reach(2);
    open_with(
        &mut rig,
        PrefsValues {
            max_multi_preview: 3,
            ..defaults()
        },
    );
    rig.click("OK", false);
    rig.pictures_reach(3);
}

#[test]
fn choosing_preferences_in_the_menu_opens_the_window_and_asks_for_frames() {
    let mut rig = Rig::new(&[], &[], 9);
    rig.frame(Vec::new());
    rig.click("Settings", false);
    rig.frame(Vec::new());
    rig.click("Preferences...", false);
    assert!(rig.app.shell.prefs_mut().is_open());
    // The counter starts at open and runs out, so the repaints stop.
    let left = |rig: &Rig| {
        rig.ctx
            .data(|d| d.get_temp::<u8>(Id::new(super::prefs::SETTLE_ID)))
    };
    assert!(left(&rig).is_some_and(|n| n > 0));
    for _ in 0..8 {
        rig.frame(Vec::new());
    }
    assert_eq!(left(&rig), Some(0));
}
