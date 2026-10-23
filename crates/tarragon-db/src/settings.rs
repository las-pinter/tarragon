//! Typed settings registry: 13 settings plus the hidden `window_state` key.
//!
//! Values store as JSON text. A stored value whose exact JSON type differs
//! from the declared type is repaired to the default. The hidden
//! `window_state` key is not a typed setting.

use serde_json::Value;

use crate::{Database, DbError};

mod window_state;

pub use window_state::{WINDOW_STATE_KEY, WindowState};

/// The number of typed settings.
pub const TYPED_SETTING_COUNT: usize = 13;

/// The accepted tile grid sizes. Only square grids from `1x1` to `4x4` count.
/// `set` rejects a value outside the set. A stored value outside the set is
/// repaired to the default on read.
pub const TILE_GRID_SIZES: &[&str] = &["1x1", "2x2", "3x3", "4x4"];

/// A typed setting key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SettingKey {
    /// Cache directory, or unset for the default location.
    CacheDir,
    /// Thumbnail cache image format.
    CacheFormat,
    /// Total cache size cap in GB. `0` disables the cap.
    CacheSizeCapGb,
    /// Whether dominant-color tagging runs.
    ColorTagEnabled,
    /// Whether exit clears the full-resolution tier.
    ClearFullResOnExit,
    /// Dominant-color palette size.
    ColorTagPaletteSize,
    /// Minimum dominant-color share.
    ColorTagMinShare,
    /// Neutral saturation threshold for color tagging.
    ColorTagNeutralSThreshold,
    /// Debug log level.
    DebugMode,
    /// Large-canvas threshold in megapixels.
    LargeCanvasThresholdMp,
    /// Maximum multi-preview count.
    MaxMultiPreview,
    /// Maximum PSD worker count.
    MaxPsdWorkers,
    /// Gallery tile grid size.
    TileGridSize,
}

impl SettingKey {
    /// Every typed key, in fixed order.
    pub const ALL: [Self; TYPED_SETTING_COUNT] = [
        Self::CacheDir,
        Self::CacheFormat,
        Self::CacheSizeCapGb,
        Self::ColorTagEnabled,
        Self::ClearFullResOnExit,
        Self::ColorTagPaletteSize,
        Self::ColorTagMinShare,
        Self::ColorTagNeutralSThreshold,
        Self::DebugMode,
        Self::LargeCanvasThresholdMp,
        Self::MaxMultiPreview,
        Self::MaxPsdWorkers,
        Self::TileGridSize,
    ];

    /// The storage key text.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CacheDir => "cache_dir",
            Self::CacheFormat => "cache_format",
            Self::CacheSizeCapGb => "cache_size_cap_gb",
            Self::ColorTagEnabled => "color_tag_enabled",
            Self::ClearFullResOnExit => "clear_full_res_on_exit",
            Self::ColorTagPaletteSize => "color_tag_palette_size",
            Self::ColorTagMinShare => "color_tag_min_share",
            Self::ColorTagNeutralSThreshold => "color_tag_neutral_s_threshold",
            Self::DebugMode => "debug_mode",
            Self::LargeCanvasThresholdMp => "large_canvas_threshold_mp",
            Self::MaxMultiPreview => "max_multi_preview",
            Self::MaxPsdWorkers => "max_psd_workers",
            Self::TileGridSize => "tile_grid_size",
        }
    }

    /// The declarative value kind for this key.
    #[must_use]
    pub const fn kind(self) -> SettingKind {
        match self {
            Self::CacheDir => SettingKind::OptionalText,
            Self::CacheFormat => SettingKind::Choice {
                default: "jpeg",
                choices: CACHE_FORMATS,
            },
            Self::CacheSizeCapGb => SettingKind::Int {
                default: 20,
                min: 0,
                max: 10_000,
            },
            Self::ColorTagEnabled | Self::ClearFullResOnExit => SettingKind::Bool { default: true },
            Self::ColorTagPaletteSize => SettingKind::Int {
                default: 8,
                min: 2,
                max: 32,
            },
            Self::ColorTagMinShare => SettingKind::Float {
                default: 0.10,
                min: 0.0,
                max: 1.0,
            },
            Self::ColorTagNeutralSThreshold => SettingKind::Float {
                default: 0.15,
                min: 0.0,
                max: 1.0,
            },
            Self::DebugMode => SettingKind::Bool { default: false },
            Self::LargeCanvasThresholdMp => SettingKind::Float {
                default: 20.0,
                min: 0.1,
                max: 1000.0,
            },
            Self::MaxMultiPreview => SettingKind::Int {
                default: 9,
                min: 1,
                max: 100,
            },
            Self::MaxPsdWorkers => SettingKind::Int {
                default: 3,
                min: 1,
                max: 8,
            },
            Self::TileGridSize => SettingKind::Choice {
                default: "3x3",
                choices: TILE_GRID_SIZES,
            },
        }
    }

    /// The default value for this key.
    #[must_use]
    pub fn default_value(self) -> SettingValue {
        self.kind().default_value()
    }
}

/// The accepted cache formats. The app writes JPEG only, so this list holds
/// one value.
pub const CACHE_FORMATS: &[&str] = &["jpeg"];

/// A declarative setting kind: the value type, the bounds, and the default.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SettingKind {
    /// Optional text. The default is unset.
    OptionalText,
    /// A fixed choice from `choices`. The default is `default`.
    Choice {
        /// The default choice.
        default: &'static str,
        /// The accepted choices.
        choices: &'static [&'static str],
    },
    /// A boolean. The default is `default`.
    Bool {
        /// The default flag.
        default: bool,
    },
    /// An integer in `min..=max`. The default is `default`.
    Int {
        /// The default number.
        default: i64,
        /// The inclusive minimum.
        min: i64,
        /// The inclusive maximum.
        max: i64,
    },
    /// A float in `min..=max`. The default is `default`.
    Float {
        /// The default number.
        default: f64,
        /// The inclusive minimum.
        min: f64,
        /// The inclusive maximum.
        max: f64,
    },
}

impl SettingKind {
    /// The default value for this kind.
    #[must_use]
    pub fn default_value(self) -> SettingValue {
        match self {
            Self::OptionalText => SettingValue::None,
            Self::Choice { default, .. } => SettingValue::Text(default.to_owned()),
            Self::Bool { default } => SettingValue::Bool(default),
            Self::Int { default, .. } => SettingValue::Int(default),
            Self::Float { default, .. } => SettingValue::Float(default),
        }
    }

    /// The declared value type name, for error messages.
    #[must_use]
    pub const fn type_name(self) -> &'static str {
        match self {
            Self::OptionalText => "path or none",
            Self::Choice { .. } => "text",
            Self::Bool { .. } => "bool",
            Self::Int { .. } => "int",
            Self::Float { .. } => "float",
        }
    }
}

/// A stored setting value.
#[derive(Debug, Clone, PartialEq)]
pub enum SettingValue {
    /// An unset optional value.
    None,
    /// A boolean.
    Bool(bool),
    /// An integer.
    Int(i64),
    /// A float.
    Float(f64),
    /// A text value.
    Text(String),
}

impl SettingValue {
    /// The given value type name, for error messages.
    #[must_use]
    pub const fn type_name(&self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Bool(_) => "bool",
            Self::Int(_) => "int",
            Self::Float(_) => "float",
            Self::Text(_) => "text",
        }
    }
}

/// A settings error.
#[derive(Debug, thiserror::Error)]
pub enum SettingsError {
    /// The value type differs from the setting's declared type.
    #[error("invalid {key}: expected {expected}, got {actual}")]
    TypeMismatch {
        /// The setting storage key.
        key: &'static str,
        /// The declared type name.
        expected: &'static str,
        /// The given type name.
        actual: &'static str,
    },
    /// The value is not one of the declared choices.
    #[error("invalid {key}: {value}. Expected one of {choices:?}.")]
    InvalidChoice {
        /// The setting storage key.
        key: &'static str,
        /// The rejected value.
        value: String,
        /// The accepted choices.
        choices: &'static [&'static str],
    },
    /// A float value is not finite.
    #[error("invalid {key}: the float value must be finite")]
    NonFinite {
        /// The setting storage key.
        key: &'static str,
    },
    /// The hidden window state text is not valid JSON.
    #[error("invalid {key}: the value must be valid JSON")]
    InvalidWindowState {
        /// The setting storage key.
        key: &'static str,
    },
    /// A database read or write failed.
    #[error(transparent)]
    Db(#[from] DbError),
}

/// Typed access to the settings in one [`Database`].
pub struct Settings<'a> {
    db: &'a Database,
}

impl<'a> Settings<'a> {
    /// Bind the registry to a database.
    #[must_use]
    pub const fn new(db: &'a Database) -> Self {
        Self { db }
    }

    /// Read one typed setting.
    ///
    /// A stored value whose exact JSON type differs from the declared type,
    /// or whose text is malformed, is repaired to the default.
    ///
    /// # Errors
    ///
    /// Returns [`DbError`] when the read fails.
    pub fn get(&self, key: SettingKey) -> Result<SettingValue, DbError> {
        let kind = key.kind();
        match self.db.setting(key.as_str())? {
            Some(text) => Ok(repair(kind, &text)),
            None => Ok(kind.default_value()),
        }
    }

    /// Write one typed setting after validation and clamping.
    ///
    /// Numeric values are clamped into the declared bounds. A wrong type or
    /// an unknown choice is refused, never coerced.
    ///
    /// # Errors
    ///
    /// Returns [`SettingsError::TypeMismatch`],
    /// [`SettingsError::InvalidChoice`], or [`SettingsError::NonFinite`] for a
    /// bad value. Returns [`SettingsError::Db`] when the write fails.
    pub fn set(&self, key: SettingKey, value: SettingValue) -> Result<(), SettingsError> {
        let stored = validate(key, key.kind(), value)?;
        let text = encode(key, stored)?;
        self.db.set_setting(key.as_str(), &text)?;
        Ok(())
    }

    /// Read the hidden `window_state` JSON text.
    ///
    /// # Errors
    ///
    /// Returns [`DbError`] when the read fails.
    pub fn window_state(&self) -> Result<Option<String>, DbError> {
        self.db.setting(WINDOW_STATE_KEY)
    }

    /// Write the hidden `window_state` JSON text.
    ///
    /// The value must parse as JSON. A malformed value is refused and is not
    /// written.
    ///
    /// # Errors
    ///
    /// Returns [`SettingsError::InvalidWindowState`] when the value is not
    /// valid JSON. Returns [`SettingsError::Db`] when the write fails.
    pub fn set_window_state(&self, value: &str) -> Result<(), SettingsError> {
        if serde_json::from_str::<Value>(value).is_err() {
            return Err(SettingsError::InvalidWindowState {
                key: WINDOW_STATE_KEY,
            });
        }
        self.db.set_setting(WINDOW_STATE_KEY, value)?;
        Ok(())
    }
}

/// Validate a value against its kind and clamp the numeric bounds.
fn validate(
    key: SettingKey,
    kind: SettingKind,
    value: SettingValue,
) -> Result<SettingValue, SettingsError> {
    match (kind, value) {
        (SettingKind::OptionalText, SettingValue::None) => Ok(SettingValue::None),
        (SettingKind::OptionalText, SettingValue::Text(text)) => Ok(SettingValue::Text(text)),
        (SettingKind::Choice { choices, .. }, SettingValue::Text(text)) => {
            if choices.contains(&text.as_str()) {
                Ok(SettingValue::Text(text))
            } else {
                Err(SettingsError::InvalidChoice {
                    key: key.as_str(),
                    value: text,
                    choices,
                })
            }
        }
        (SettingKind::Bool { .. }, SettingValue::Bool(flag)) => Ok(SettingValue::Bool(flag)),
        (SettingKind::Int { min, max, .. }, SettingValue::Int(number)) => {
            Ok(SettingValue::Int(number.clamp(min, max)))
        }
        (SettingKind::Float { min, max, .. }, SettingValue::Float(number)) => {
            if number.is_finite() {
                Ok(SettingValue::Float(number.clamp(min, max)))
            } else {
                Err(SettingsError::NonFinite { key: key.as_str() })
            }
        }
        (kind, value) => Err(SettingsError::TypeMismatch {
            key: key.as_str(),
            expected: kind.type_name(),
            actual: value.type_name(),
        }),
    }
}

/// Encode a validated value as JSON text.
fn encode(key: SettingKey, value: SettingValue) -> Result<String, SettingsError> {
    match value {
        SettingValue::None => Ok("null".to_owned()),
        SettingValue::Bool(flag) => Ok(flag.to_string()),
        SettingValue::Int(number) => Ok(number.to_string()),
        SettingValue::Float(number) => {
            let encoded = serde_json::Number::from_f64(number)
                .ok_or(SettingsError::NonFinite { key: key.as_str() })?;
            Ok(encoded.to_string())
        }
        SettingValue::Text(text) => Ok(Value::String(text).to_string()),
    }
}

/// Parse a stored value and apply the read-repair, clamp, and choice rules.
///
/// The exact-type rule uses the JSON type, not a coercion. So a stored `true`
/// or `3.5` is wrong for an int key, and a stored `5` is wrong for a float key.
fn repair(kind: SettingKind, text: &str) -> SettingValue {
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
