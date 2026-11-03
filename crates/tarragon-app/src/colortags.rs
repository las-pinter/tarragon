//! Colour tag settings and names for the colour tagging step.

#![cfg_attr(not(test), expect(dead_code, reason = "no caller in the binary yet"))]

use tarragon_core::color::TRANSPARENT_TAG;
use tarragon_db::Database;
use tarragon_db::settings::{SettingKey, SettingValue, Settings};
use tarragon_render::color::{ColorTagConfig, ColorTags};

use crate::filtering::colors_enabled;

/// Read the colour tag config from the settings.
///
/// Returns `None` when colour tagging is off. A setting that cannot be read
/// gives the default for that field and is logged.
pub fn read_config(db: &Database) -> Option<ColorTagConfig> {
    if !colors_enabled(db) {
        return None;
    }
    let settings = Settings::new(db);
    let default = ColorTagConfig::default();
    Some(ColorTagConfig {
        palette_size: match read(&settings, SettingKey::ColorTagPaletteSize) {
            Some(SettingValue::Int(value)) => {
                usize::try_from(value).unwrap_or(default.palette_size)
            }
            _ => default.palette_size,
        },
        min_share: float(&settings, SettingKey::ColorTagMinShare, default.min_share),
        neutral_s_threshold: float(
            &settings,
            SettingKey::ColorTagNeutralSThreshold,
            default.neutral_s_threshold,
        ),
    })
}

/// Return the tag names of one image: the bucket names, then `transparent`.
pub fn tag_names(tags: &ColorTags) -> Vec<String> {
    let mut names: Vec<String> = tags
        .buckets
        .iter()
        .map(|bucket| bucket.as_str().to_owned())
        .collect();
    if tags.transparent {
        names.push(TRANSPARENT_TAG.to_owned());
    }
    names
}

/// Read one setting, and log it when it cannot be read.
fn read(settings: &Settings<'_>, key: SettingKey) -> Option<SettingValue> {
    match settings.get(key) {
        Ok(value) => Some(value),
        Err(error) => {
            tracing::warn!("cannot read the setting {}: {error}", key.as_str());
            None
        }
    }
}

/// Read a float setting, or `default` when it cannot be read.
fn float(settings: &Settings<'_>, key: SettingKey, default: f64) -> f64 {
    match read(settings, key) {
        Some(SettingValue::Float(value)) => value,
        _ => default,
    }
}

#[cfg(test)]
mod tests;
