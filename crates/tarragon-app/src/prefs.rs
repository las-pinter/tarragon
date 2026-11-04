//! The Preferences values: read from the settings and written back.
//!
//! When a saved value takes effect:
//! - `debug`, `max_multi_preview`, and `color_tag_enabled` apply at once.
//! - The other three colour settings apply at the next folder open.
//! - `clear_full_res` is read at exit.
//! - `cache_cap_gb` and `cache_dir` apply at the next start.
//! - `cache_format`, `tile_grid`, `max_psd_workers`, and `large_canvas_mp` are
//!   saved, but no code reads them yet.

use tarragon_db::Database;
use tarragon_db::settings::{SettingKey, SettingValue, Settings};
use tarragon_ui::prefs::PrefsValues;

/// Read the 13 settings. A key that cannot be read logs a warning and gives
/// its default.
pub fn load(db: &Database) -> PrefsValues {
    let settings = Settings::new(db);
    PrefsValues {
        max_psd_workers: read(&settings, SettingKey::MaxPsdWorkers, uint),
        max_multi_preview: read(&settings, SettingKey::MaxMultiPreview, uint),
        large_canvas_mp: read(&settings, SettingKey::LargeCanvasThresholdMp, float),
        tile_grid: read(&settings, SettingKey::TileGridSize, text),
        color_tag_enabled: read(&settings, SettingKey::ColorTagEnabled, flag),
        palette_size: read(&settings, SettingKey::ColorTagPaletteSize, uint),
        min_share: read(&settings, SettingKey::ColorTagMinShare, float),
        neutral_s: read(&settings, SettingKey::ColorTagNeutralSThreshold, float),
        cache_dir: cache_dir(&settings),
        cache_format: read(&settings, SettingKey::CacheFormat, text),
        clear_full_res: read(&settings, SettingKey::ClearFullResOnExit, flag),
        cache_cap_gb: read(&settings, SettingKey::CacheSizeCapGb, uint),
        debug: read(&settings, SettingKey::DebugMode, flag),
    }
}

/// The settings to write for `values`. A blank cache folder means the default
/// one, so it is stored as unset.
pub fn pairs(values: &PrefsValues) -> Vec<(SettingKey, SettingValue)> {
    let dir = values
        .cache_dir
        .as_deref()
        .map(str::trim)
        .filter(|dir| !dir.is_empty())
        .map_or(SettingValue::None, |dir| SettingValue::Text(dir.to_owned()));
    vec![
        (
            SettingKey::MaxPsdWorkers,
            SettingValue::Int(i64::from(values.max_psd_workers)),
        ),
        (
            SettingKey::MaxMultiPreview,
            SettingValue::Int(i64::from(values.max_multi_preview)),
        ),
        (
            SettingKey::LargeCanvasThresholdMp,
            SettingValue::Float(decimal(values.large_canvas_mp)),
        ),
        (
            SettingKey::TileGridSize,
            SettingValue::Text(values.tile_grid.clone()),
        ),
        (
            SettingKey::ColorTagEnabled,
            SettingValue::Bool(values.color_tag_enabled),
        ),
        (
            SettingKey::ColorTagPaletteSize,
            SettingValue::Int(i64::from(values.palette_size)),
        ),
        (
            SettingKey::ColorTagMinShare,
            SettingValue::Float(decimal(values.min_share)),
        ),
        (
            SettingKey::ColorTagNeutralSThreshold,
            SettingValue::Float(decimal(values.neutral_s)),
        ),
        (SettingKey::CacheDir, dir),
        (
            SettingKey::CacheFormat,
            SettingValue::Text(values.cache_format.clone()),
        ),
        (
            SettingKey::ClearFullResOnExit,
            SettingValue::Bool(values.clear_full_res),
        ),
        (
            SettingKey::CacheSizeCapGb,
            SettingValue::Int(i64::from(values.cache_cap_gb)),
        ),
        (SettingKey::DebugMode, SettingValue::Bool(values.debug)),
    ]
}

/// A float as its shortest decimal text read back as `f64`, so `0.1` is stored
/// as `0.1` and not as `0.10000000149011612`.
fn decimal(value: f32) -> f64 {
    value
        .to_string()
        .parse()
        .unwrap_or_else(|_| f64::from(value))
}

/// Read one key and pick its value. A failed read, or a value of another kind,
/// falls back to the default of the key.
fn read<T: Default>(
    settings: &Settings<'_>,
    key: SettingKey,
    pick: fn(&SettingValue) -> Option<T>,
) -> T {
    let value = settings.get(key).unwrap_or_else(|error| {
        tracing::warn!("cannot read the setting {}: {error}", key.as_str());
        key.default_value()
    });
    pick(&value)
        .or_else(|| pick(&key.default_value()))
        .unwrap_or_default()
}

/// The cache folder, or `None` when it is unset or cannot be read.
fn cache_dir(settings: &Settings<'_>) -> Option<String> {
    match settings.get(SettingKey::CacheDir) {
        Ok(SettingValue::Text(dir)) => Some(dir),
        Ok(_) => None,
        Err(error) => {
            tracing::warn!("cannot read the cache folder setting: {error}");
            None
        }
    }
}

fn uint(value: &SettingValue) -> Option<u32> {
    match value {
        SettingValue::Int(number) => u32::try_from(*number).ok(),
        _ => None,
    }
}

// The registry bounds keep every float far inside the `f32` range.
#[allow(clippy::cast_possible_truncation)]
fn float(value: &SettingValue) -> Option<f32> {
    match value {
        SettingValue::Float(number) => Some(*number as f32),
        _ => None,
    }
}

fn flag(value: &SettingValue) -> Option<bool> {
    match value {
        SettingValue::Bool(on) => Some(*on),
        _ => None,
    }
}

fn text(value: &SettingValue) -> Option<String> {
    match value {
        SettingValue::Text(text) => Some(text.clone()),
        _ => None,
    }
}
