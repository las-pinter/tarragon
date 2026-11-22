//! The read-repair of stored setting values.

use serde_json::Value;

use super::{SettingKind, SettingValue};

/// Whether the value read from `text` differs from what `text` holds: the text
/// is malformed, has the wrong type, or holds a number out of bounds.
pub(crate) fn was_repaired(text: &str, value: &SettingValue) -> bool {
    let read = match value {
        SettingValue::None => Value::Null,
        SettingValue::Bool(flag) => Value::Bool(*flag),
        SettingValue::Int(number) => Value::from(*number),
        SettingValue::Float(number) => Value::from(*number),
        SettingValue::Text(text) => Value::String(text.clone()),
    };
    serde_json::from_str::<Value>(text).map_or(true, |stored| stored != read)
}

/// Parse a stored value and apply the read-repair, clamp, and choice rules.
///
/// The exact-type rule uses the JSON type, not a coercion. So a stored `true`
/// or `3.5` is wrong for an int key, and a stored `5` is wrong for a float key.
pub(super) fn repair(kind: SettingKind, text: &str) -> SettingValue {
    let Ok(value) = serde_json::from_str::<Value>(text) else {
        return kind.default_value();
    };
    match kind {
        SettingKind::OptionalText => match value {
            Value::String(text) => SettingValue::Text(text),
            Value::Null => SettingValue::None,
            _ => kind.default_value(),
        },
        SettingKind::Choice { choices, .. } => match value {
            Value::String(text) if choices.contains(&text.as_str()) => SettingValue::Text(text),
            _ => kind.default_value(),
        },
        SettingKind::Bool { .. } => match value {
            Value::Bool(flag) => SettingValue::Bool(flag),
            _ => kind.default_value(),
        },
        SettingKind::Int { min, max, .. } => match value.as_i64() {
            // `as_i64` is `None` for a JSON real, so an int key rejects a float.
            Some(number) => SettingValue::Int(number.clamp(min, max)),
            None => kind.default_value(),
        },
        SettingKind::Float { min, max, .. } => match value.as_f64() {
            // `is_f64` is false for a JSON int, so a float key rejects an int.
            Some(number) if value.is_f64() && number.is_finite() => {
                SettingValue::Float(number.clamp(min, max))
            }
            _ => kind.default_value(),
        },
    }
}
