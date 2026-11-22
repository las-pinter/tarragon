//! Which stored setting values count as repaired when read, so the read logs a
//! warning, and the store of a repair.

use crate::settings::{SettingKey, SettingValue, Settings, was_repaired};

use super::support::TempDb;

#[test]
fn a_value_read_as_stored_is_not_a_repair() {
    let cases = [
        ("null", SettingValue::None),
        ("true", SettingValue::Bool(true)),
        ("25", SettingValue::Int(25)),
        ("0.25", SettingValue::Float(0.25)),
        ("\"jpeg\"", SettingValue::Text("jpeg".to_owned())),
    ];
    for (text, value) in cases {
        assert!(!was_repaired(text, &value), "{text}");
    }
}

#[test]
fn malformed_text_is_a_repair() {
    assert!(was_repaired("{not-json", &SettingValue::Int(20)));
}

#[test]
fn a_wrong_type_read_as_the_default_is_a_repair() {
    assert!(was_repaired("\"twenty\"", &SettingValue::Int(20)));
    assert!(was_repaired("3", &SettingValue::Float(0.25)));
}

#[test]
fn a_clamped_number_is_a_repair() {
    assert!(was_repaired("9999", &SettingValue::Int(100)));
}

#[test]
fn a_repaired_value_is_stored_for_the_next_read() {
    let temp = TempDb::new();
    let key = SettingKey::CacheSizeCapGb;
    temp.db
        .set_setting(key.as_str(), "\"twenty\"")
        .expect("store a bad value");

    let value = Settings::new(&temp.db).get(key).expect("read");

    let stored = temp.db.setting(key.as_str()).expect("read the text");
    let stored = stored.expect("a stored value");
    assert!(!was_repaired(&stored, &value), "stored {stored}");
}
