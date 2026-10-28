//! Tests for the hidden window-state blob.

use std::collections::BTreeSet;

use serde_json::Value;
use tarragon_core::sorting::SortMode;

use crate::DbError;
use crate::settings::{Settings, SettingsError, WINDOW_STATE_KEY, WindowState};

use super::support::TempDb;

#[test]
fn window_state_defaults_to_unset() {
    let temp = TempDb::new();
    let settings = Settings::new(&temp.db);
    assert_eq!(settings.window_state().expect("read"), None);
}

#[test]
fn window_state_round_trips_json_text() {
    let temp = TempDb::new();
    let settings = Settings::new(&temp.db);
    let state = r#"{"layout":"library","maximized":true,"sort_mode":"date"}"#;
    settings.set_window_state(state).expect("write the state");
    assert_eq!(
        settings.window_state().expect("read").as_deref(),
        Some(state)
    );
}

#[test]
fn window_state_rejects_malformed_json_and_does_not_persist() {
    let temp = TempDb::new();
    let settings = Settings::new(&temp.db);
    let error = settings
        .set_window_state("{not-json")
        .expect_err("malformed JSON is refused");
    assert!(
        matches!(error, SettingsError::InvalidWindowState { .. }),
        "{error}"
    );
    assert_eq!(settings.window_state().expect("read"), None);
}

/// A full window state with one extra key that this version does not name.
fn window_state_sample() -> WindowState {
    let mut state = WindowState {
        layout: Some("library".to_owned()),
        maximized: Some(true),
        sort_mode: Some("date".to_owned()),
        ..WindowState::default()
    };
    state
        .extra
        .insert("scroll_y".to_owned(), Value::from(120.0));
    state
}

#[test]
fn typed_window_state_round_trips_through_the_database() {
    let temp = TempDb::new();
    let settings = Settings::new(&temp.db);
    assert_eq!(settings.window_state_value().expect("read"), None);

    let state = window_state_sample();
    settings
        .set_window_state_value(&state)
        .expect("write the state");
    assert_eq!(
        settings.window_state_value().expect("read"),
        Some(state),
        "layout, maximized flag, and sort mode survive the round trip"
    );
}

#[test]
fn typed_window_state_keeps_an_unknown_key() {
    let temp = TempDb::new();
    let settings = Settings::new(&temp.db);
    settings
        .set_window_state_value(&window_state_sample())
        .expect("write the state");

    let state = settings
        .window_state_value()
        .expect("read")
        .expect("a stored state");
    assert_eq!(state.extra.get("scroll_y"), Some(&Value::from(120.0)));
}

#[test]
fn typed_window_state_names_three_keys_and_keeps_the_extras() {
    let temp = TempDb::new();
    let settings = Settings::new(&temp.db);
    settings
        .set_window_state_value(&window_state_sample())
        .expect("write the state");

    let text = settings.window_state().expect("read").expect("stored text");
    let value: Value = serde_json::from_str(&text).expect("the text is JSON");
    let keys: BTreeSet<&str> = value
        .as_object()
        .expect("the text is an object")
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(
        keys,
        BTreeSet::from(["layout", "maximized", "scroll_y", "sort_mode"]),
        "three named keys plus the extra key"
    );
}

#[test]
fn strict_window_state_reports_a_malformed_stored_value() {
    let temp = TempDb::new();
    let settings = Settings::new(&temp.db);
    temp.db
        .set_setting(WINDOW_STATE_KEY, "[1, 2, 3]")
        .expect("write raw text");

    let error = settings
        .window_state_strict()
        .expect_err("an array is not a window state");
    assert!(
        matches!(error, SettingsError::Db(DbError::WindowState(_))),
        "{error}"
    );
}

#[test]
fn a_malformed_window_state_opens_with_the_default() {
    let temp = TempDb::new();
    let settings = Settings::new(&temp.db);
    temp.db
        .set_setting(WINDOW_STATE_KEY, "[1, 2, 3]")
        .expect("write raw text");

    assert_eq!(
        settings.window_state_value().expect("read"),
        Some(WindowState::default())
    );
    let (_, repaired) = settings
        .window_state_repaired()
        .expect("read")
        .expect("stored");
    assert!(repaired);
}

#[test]
fn a_wrong_typed_field_keeps_the_valid_keys_and_the_extras() {
    let temp = TempDb::new();
    let settings = Settings::new(&temp.db);
    temp.db
        .set_setting(
            WINDOW_STATE_KEY,
            r#"{"layout":7,"maximized":true,"sort_mode":"date","scroll_y":9}"#,
        )
        .expect("write raw text");

    let (state, repaired) = settings
        .window_state_repaired()
        .expect("read")
        .expect("stored");
    assert!(repaired);
    assert_eq!(state.layout, None);
    assert_eq!(state.maximized, Some(true));
    assert_eq!(state.sort_mode(), SortMode::Date);
    assert_eq!(state.extra.get("scroll_y"), Some(&Value::from(9)));
}

#[test]
fn a_valid_window_state_reports_no_repair() {
    let temp = TempDb::new();
    let settings = Settings::new(&temp.db);
    settings
        .set_window_state_value(&window_state_sample())
        .expect("write the state");

    let (_, repaired) = settings
        .window_state_repaired()
        .expect("read")
        .expect("stored");
    assert!(!repaired);
}

#[test]
fn typed_window_state_stores_the_date_sort_mode() {
    let temp = TempDb::new();
    let settings = Settings::new(&temp.db);

    let mut state = WindowState::default();
    assert_eq!(
        state.sort_mode(),
        SortMode::Natural,
        "unset reads as default"
    );
    state.set_sort_mode(SortMode::Date);
    assert_eq!(state.sort_mode, Some("date".to_owned()));
    settings
        .set_window_state_value(&state)
        .expect("write the state");

    let stored = settings
        .window_state_value()
        .expect("read")
        .expect("a stored state");
    assert_eq!(stored.sort_mode(), SortMode::Date);
}

#[test]
fn an_unknown_stored_sort_mode_reads_as_natural() {
    let state = WindowState {
        sort_mode: Some("size".to_owned()),
        ..WindowState::default()
    };
    assert_eq!(state.sort_mode(), SortMode::Natural);
}

#[test]
fn the_maximized_flag_round_trips_through_the_database() {
    let temp = TempDb::new();
    let settings = Settings::new(&temp.db);
    for flag in [Some(true), Some(false)] {
        let state = WindowState {
            maximized: flag,
            ..WindowState::default()
        };
        settings
            .set_window_state_value(&state)
            .expect("write the state");
        let stored = settings
            .window_state_value()
            .expect("read")
            .expect("a stored state");
        assert_eq!(stored.maximized, flag);
    }
}

#[test]
fn an_old_row_without_the_maximized_key_reads_as_none() {
    let text = r#"{"layout":"library","sort_mode":"date"}"#;
    let state: WindowState = serde_json::from_str(text).expect("decode");
    assert_eq!(state.maximized, None);
    assert!(!state.extra.contains_key("maximized"));
}

#[test]
fn a_wrong_typed_maximized_flag_is_repaired_to_none() {
    let temp = TempDb::new();
    let settings = Settings::new(&temp.db);
    temp.db
        .set_setting(
            WINDOW_STATE_KEY,
            r#"{"maximized":"yes","layout":"library"}"#,
        )
        .expect("write raw text");

    let (state, repaired) = settings
        .window_state_repaired()
        .expect("read")
        .expect("stored");
    assert!(repaired);
    assert_eq!(state.maximized, None);
    assert_eq!(state.layout.as_deref(), Some("library"));
}

#[test]
fn an_old_row_with_geometry_and_frame_shift_loses_both_keys_on_the_next_save() {
    let temp = TempDb::new();
    let settings = Settings::new(&temp.db);
    temp.db
        .set_setting(
            WINDOW_STATE_KEY,
            r#"{"layout":"library","geometry":[1,2,3,4],"frame_shift":[0,28],"scroll_y":9}"#,
        )
        .expect("write raw text");

    let (state, repaired) = settings
        .window_state_repaired()
        .expect("read")
        .expect("stored");
    assert!(!repaired);
    assert_eq!(state.layout.as_deref(), Some("library"));
    settings
        .set_window_state_value(&state)
        .expect("write the state");

    let text = settings.window_state().expect("read").expect("stored text");
    let value: Value = serde_json::from_str(&text).expect("the text is JSON");
    let object = value.as_object().expect("the text is an object");
    assert!(!object.contains_key("geometry"));
    assert!(!object.contains_key("frame_shift"));
    assert_eq!(object.get("scroll_y"), Some(&Value::from(9)));
}
