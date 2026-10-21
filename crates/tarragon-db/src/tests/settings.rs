use std::collections::BTreeSet;

use crate::settings::{SettingKey, SettingValue, Settings, SettingsError, WINDOW_STATE_KEY};

use super::support::TempDb;

/// A valid value for every key, used by the round-trip and default tests.
fn settings_sample_value(key: SettingKey) -> SettingValue {
    match key {
        SettingKey::CacheDir => SettingValue::Text("/tmp/tarragon-cache".to_owned()),
        SettingKey::CacheFormat => SettingValue::Text("jpeg".to_owned()),
        SettingKey::CacheSizeCapGb => SettingValue::Int(50),
        SettingKey::ColorTagEnabled | SettingKey::ClearFullResOnExit => SettingValue::Bool(false),
        SettingKey::ColorTagPaletteSize => SettingValue::Int(16),
        SettingKey::ColorTagMinShare => SettingValue::Float(0.25),
        SettingKey::ColorTagNeutralSThreshold => SettingValue::Float(0.30),
        SettingKey::DebugMode => SettingValue::Bool(true),
        SettingKey::LargeCanvasThresholdMp => SettingValue::Float(50.5),
        SettingKey::MaxMultiPreview => SettingValue::Int(25),
        SettingKey::MaxPsdWorkers => SettingValue::Int(5),
        SettingKey::TileGridSize => SettingValue::Text("2x2".to_owned()),
    }
}

#[test]
fn settings_have_thirteen_unique_typed_keys() {
    assert_eq!(SettingKey::ALL.len(), 13);
    let names: BTreeSet<&str> = SettingKey::ALL.iter().map(|key| key.as_str()).collect();
    assert_eq!(names.len(), SettingKey::ALL.len());
}

#[test]
fn settings_load_every_default_on_a_fresh_database() {
    let temp = TempDb::new();
    let settings = Settings::new(&temp.db);
    for key in SettingKey::ALL {
        assert_eq!(
            settings.get(key).expect("read the default"),
            key.default_value(),
            "wrong default for {}",
            key.as_str()
        );
    }
}

#[test]
fn settings_round_trip_a_set_value_for_every_key() {
    let temp = TempDb::new();
    let settings = Settings::new(&temp.db);
    for key in SettingKey::ALL {
        let value = settings_sample_value(key);
        settings.set(key, value.clone()).expect("set the value");
        assert_eq!(
            settings.get(key).expect("read the value"),
            value,
            "no round trip for {}",
            key.as_str()
        );
    }
}

#[test]
fn cache_format_defaults_to_jpeg() {
    let temp = TempDb::new();
    let settings = Settings::new(&temp.db);
    assert_eq!(
        settings.get(SettingKey::CacheFormat).expect("read"),
        SettingValue::Text("jpeg".to_owned())
    );
}

#[test]
fn cache_size_cap_gb_defaults_to_twenty() {
    let temp = TempDb::new();
    let settings = Settings::new(&temp.db);
    assert_eq!(
        settings.get(SettingKey::CacheSizeCapGb).expect("read"),
        SettingValue::Int(20)
    );
}

#[test]
fn cache_format_rejects_an_unknown_choice() {
    let temp = TempDb::new();
    let settings = Settings::new(&temp.db);
    let error = settings
        .set(
            SettingKey::CacheFormat,
            SettingValue::Text("png".to_owned()),
        )
        .expect_err("png is not a valid choice");
    assert!(
        matches!(error, SettingsError::InvalidChoice { .. }),
        "{error}"
    );
    assert_eq!(temp.db.setting("cache_format").expect("read raw"), None);
}

#[test]
fn cache_format_stored_unknown_choice_falls_back_to_jpeg() {
    let temp = TempDb::new();
    temp.db
        .set_setting("cache_format", "\"PNG\"")
        .expect("seed the bad value");
    let settings = Settings::new(&temp.db);
    assert_eq!(
        settings.get(SettingKey::CacheFormat).expect("read"),
        SettingValue::Text("jpeg".to_owned())
    );
}

#[test]
fn settings_clamp_numeric_values_above_the_maximum() {
    let temp = TempDb::new();
    let settings = Settings::new(&temp.db);
    settings
        .set(SettingKey::ColorTagPaletteSize, SettingValue::Int(100))
        .expect("set");
    assert_eq!(
        settings.get(SettingKey::ColorTagPaletteSize).expect("read"),
        SettingValue::Int(32)
    );
}

#[test]
fn settings_clamp_numeric_values_below_the_minimum() {
    let temp = TempDb::new();
    let settings = Settings::new(&temp.db);
    settings
        .set(SettingKey::ColorTagPaletteSize, SettingValue::Int(0))
        .expect("set");
    assert_eq!(
        settings.get(SettingKey::ColorTagPaletteSize).expect("read"),
        SettingValue::Int(2)
    );
}

#[test]
fn settings_clamp_a_stored_float_above_the_maximum() {
    let temp = TempDb::new();
    let settings = Settings::new(&temp.db);
    settings
        .set(
            SettingKey::LargeCanvasThresholdMp,
            SettingValue::Float(9999.0),
        )
        .expect("set");
    assert_eq!(
        settings
            .get(SettingKey::LargeCanvasThresholdMp)
            .expect("read"),
        SettingValue::Float(1000.0)
    );
}

#[test]
fn cache_size_cap_gb_clamps_to_ten_thousand() {
    let temp = TempDb::new();
    let settings = Settings::new(&temp.db);
    settings
        .set(SettingKey::CacheSizeCapGb, SettingValue::Int(20_000))
        .expect("set");
    assert_eq!(
        settings.get(SettingKey::CacheSizeCapGb).expect("read"),
        SettingValue::Int(10_000)
    );
}

#[test]
fn cache_size_cap_gb_zero_disables_the_cap() {
    let temp = TempDb::new();
    let settings = Settings::new(&temp.db);
    settings
        .set(SettingKey::CacheSizeCapGb, SettingValue::Int(0))
        .expect("set");
    assert_eq!(
        settings.get(SettingKey::CacheSizeCapGb).expect("read"),
        SettingValue::Int(0)
    );
}

#[test]
fn settings_clamp_a_stored_value_out_of_range_on_read() {
    let temp = TempDb::new();
    temp.db
        .set_setting("color_tag_palette_size", "99")
        .expect("seed the bad value");
    let settings = Settings::new(&temp.db);
    assert_eq!(
        settings.get(SettingKey::ColorTagPaletteSize).expect("read"),
        SettingValue::Int(32)
    );
}

#[test]
fn settings_repair_a_stored_bool_for_an_int_key() {
    let temp = TempDb::new();
    temp.db
        .set_setting("max_psd_workers", "true")
        .expect("seed the bad value");
    let settings = Settings::new(&temp.db);
    assert_eq!(
        settings.get(SettingKey::MaxPsdWorkers).expect("read"),
        SettingValue::Int(3)
    );
}

#[test]
fn settings_repair_a_stored_float_for_an_int_key() {
    let temp = TempDb::new();
    temp.db
        .set_setting("max_psd_workers", "7.5")
        .expect("seed the bad value");
    let settings = Settings::new(&temp.db);
    assert_eq!(
        settings.get(SettingKey::MaxPsdWorkers).expect("read"),
        SettingValue::Int(3)
    );
}

#[test]
fn settings_repair_a_stored_int_for_a_float_key() {
    let temp = TempDb::new();
    temp.db
        .set_setting("large_canvas_threshold_mp", "50")
        .expect("seed the bad value");
    let settings = Settings::new(&temp.db);
    assert_eq!(
        settings
            .get(SettingKey::LargeCanvasThresholdMp)
            .expect("read"),
        SettingValue::Float(20.0)
    );
}

#[test]
fn settings_repair_malformed_stored_json_to_the_default() {
    let temp = TempDb::new();
    temp.db
        .set_setting("debug_mode", "{not-json")
        .expect("seed the bad value");
    let settings = Settings::new(&temp.db);
    assert_eq!(
        settings.get(SettingKey::DebugMode).expect("read"),
        SettingValue::Bool(false)
    );
}

#[test]
fn set_rejects_a_float_for_an_int_setting_and_does_not_persist() {
    let temp = TempDb::new();
    let settings = Settings::new(&temp.db);
    let error = settings
        .set(SettingKey::MaxPsdWorkers, SettingValue::Float(4.5))
        .expect_err("a float is not an int");
    assert!(
        matches!(error, SettingsError::TypeMismatch { .. }),
        "{error}"
    );
    assert_eq!(temp.db.setting("max_psd_workers").expect("read raw"), None);
}

#[test]
fn set_rejects_an_int_for_a_bool_setting_and_does_not_persist() {
    let temp = TempDb::new();
    let settings = Settings::new(&temp.db);
    let error = settings
        .set(SettingKey::DebugMode, SettingValue::Int(1))
        .expect_err("an int is not a bool");
    assert!(
        matches!(error, SettingsError::TypeMismatch { .. }),
        "{error}"
    );
    assert_eq!(temp.db.setting("debug_mode").expect("read raw"), None);
}

#[test]
fn set_rejects_a_string_for_an_int_setting_and_does_not_persist() {
    let temp = TempDb::new();
    let settings = Settings::new(&temp.db);
    let error = settings
        .set(
            SettingKey::MaxPsdWorkers,
            SettingValue::Text("abc".to_owned()),
        )
        .expect_err("a string is not an int");
    assert!(
        matches!(error, SettingsError::TypeMismatch { .. }),
        "{error}"
    );
    assert_eq!(temp.db.setting("max_psd_workers").expect("read raw"), None);
}

#[test]
fn set_persists_the_value_as_json_text() {
    let temp = TempDb::new();
    let settings = Settings::new(&temp.db);
    settings
        .set(SettingKey::MaxPsdWorkers, SettingValue::Int(5))
        .expect("set the int");
    settings
        .set(
            SettingKey::CacheFormat,
            SettingValue::Text("jpeg".to_owned()),
        )
        .expect("set the string");
    assert_eq!(
        temp.db.setting("max_psd_workers").expect("read raw"),
        Some("5".to_owned())
    );
    assert_eq!(
        temp.db.setting("cache_format").expect("read raw"),
        Some("\"jpeg\"".to_owned())
    );
}

#[test]
fn tile_grid_accepts_one_to_four() {
    let temp = TempDb::new();
    let settings = Settings::new(&temp.db);
    for size in ["1x1", "2x2", "3x3", "4x4"] {
        settings
            .set(
                SettingKey::TileGridSize,
                SettingValue::Text(size.to_owned()),
            )
            .expect("set the grid size");
        assert_eq!(
            settings.get(SettingKey::TileGridSize).expect("read"),
            SettingValue::Text(size.to_owned())
        );
    }
}

#[test]
fn tile_grid_rejects_five_by_five_and_does_not_persist() {
    let temp = TempDb::new();
    let settings = Settings::new(&temp.db);
    let error = settings
        .set(
            SettingKey::TileGridSize,
            SettingValue::Text("5x5".to_owned()),
        )
        .expect_err("5x5 is out of range");
    assert!(
        matches!(error, SettingsError::InvalidChoice { .. }),
        "{error}"
    );
    assert_eq!(temp.db.setting("tile_grid_size").expect("read raw"), None);
}

#[test]
fn tile_grid_rejects_an_asymmetric_grid() {
    let temp = TempDb::new();
    let settings = Settings::new(&temp.db);
    let error = settings
        .set(
            SettingKey::TileGridSize,
            SettingValue::Text("4x2".to_owned()),
        )
        .expect_err("4x2 is not square");
    assert!(
        matches!(error, SettingsError::InvalidChoice { .. }),
        "{error}"
    );
}

#[test]
fn tile_grid_repairs_a_stored_out_of_range_grid() {
    let temp = TempDb::new();
    temp.db
        .set_setting("tile_grid_size", "\"99x99\"")
        .expect("seed the bad value");
    let settings = Settings::new(&temp.db);
    assert_eq!(
        settings.get(SettingKey::TileGridSize).expect("read"),
        SettingValue::Text("3x3".to_owned())
    );
}

#[test]
fn settings_accept_the_max_psd_workers_bounds() {
    let temp = TempDb::new();
    let settings = Settings::new(&temp.db);
    for workers in [1, 8] {
        settings
            .set(SettingKey::MaxPsdWorkers, SettingValue::Int(workers))
            .expect("set the workers");
        assert_eq!(
            settings.get(SettingKey::MaxPsdWorkers).expect("read"),
            SettingValue::Int(workers)
        );
    }
}

#[test]
fn settings_accept_the_max_multi_preview_bounds() {
    let temp = TempDb::new();
    let settings = Settings::new(&temp.db);
    for previews in [1, 100] {
        settings
            .set(SettingKey::MaxMultiPreview, SettingValue::Int(previews))
            .expect("set the previews");
        assert_eq!(
            settings.get(SettingKey::MaxMultiPreview).expect("read"),
            SettingValue::Int(previews)
        );
    }
}

#[test]
fn settings_accept_the_cache_size_cap_gb_maximum() {
    let temp = TempDb::new();
    let settings = Settings::new(&temp.db);
    settings
        .set(SettingKey::CacheSizeCapGb, SettingValue::Int(10_000))
        .expect("set the cap");
    assert_eq!(
        settings.get(SettingKey::CacheSizeCapGb).expect("read"),
        SettingValue::Int(10_000)
    );
}

#[test]
fn cache_format_rejects_an_uppercase_choice() {
    let temp = TempDb::new();
    let settings = Settings::new(&temp.db);
    let error = settings
        .set(
            SettingKey::CacheFormat,
            SettingValue::Text("JPEG".to_owned()),
        )
        .expect_err("JPEG is not a valid choice");
    assert!(
        matches!(error, SettingsError::InvalidChoice { .. }),
        "{error}"
    );
    assert_eq!(temp.db.setting("cache_format").expect("read raw"), None);
}

#[test]
fn set_rejects_a_non_finite_float_and_does_not_persist() {
    let temp = TempDb::new();
    let settings = Settings::new(&temp.db);
    for value in [f64::INFINITY, f64::NAN] {
        let error = settings
            .set(
                SettingKey::LargeCanvasThresholdMp,
                SettingValue::Float(value),
            )
            .expect_err("a non-finite float is not valid");
        assert!(matches!(error, SettingsError::NonFinite { .. }), "{error}");
    }
    assert_eq!(
        temp.db
            .setting("large_canvas_threshold_mp")
            .expect("read raw"),
        None
    );
}

#[test]
fn window_state_is_not_a_typed_setting() {
    let names: BTreeSet<&str> = SettingKey::ALL.iter().map(|key| key.as_str()).collect();
    assert!(!names.contains(WINDOW_STATE_KEY));
}
