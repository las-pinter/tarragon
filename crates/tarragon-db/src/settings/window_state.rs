//! The hidden window-state blob: layout, geometry, and sort mode.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tarragon_core::sorting::SortMode;

use super::{Settings, SettingsError};
use crate::DbError;

/// The storage key for the hidden window state JSON blob.
pub const WINDOW_STATE_KEY: &str = "window_state";

/// The retained window layout, geometry, and sort mode.
///
/// Unknown keys stay in [`WindowState::extra`], so a round trip loses nothing.
/// A named key that is absent from the stored text reads as `None`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct WindowState {
    /// The dock layout name, or none.
    pub layout: Option<String>,
    /// The window rectangle as `[x, y, width, height]`, or none.
    pub geometry: Option<[i64; 4]>,
    /// The gallery sort mode name, or none.
    pub sort_mode: Option<String>,
    /// Keys this version does not name. They survive a round trip.
    #[serde(flatten)]
    pub extra: serde_json::Map<String, Value>,
}

impl WindowState {
    /// The stored sort mode. An absent or unknown name reads as the default.
    #[must_use]
    pub fn sort_mode(&self) -> SortMode {
        self.sort_mode
            .as_deref()
            .and_then(|name| name.parse::<SortMode>().ok())
            .unwrap_or_default()
    }

    /// Store the sort mode under its storage name.
    pub fn set_sort_mode(&mut self, mode: SortMode) {
        self.sort_mode = Some(mode.as_str().to_owned());
    }
}

impl Settings<'_> {
    /// Read the hidden `window_state` value as a typed [`WindowState`].
    ///
    /// An unset key gives `None`. A stored value with a wrong-typed field is
    /// repaired: the bad field reads as unset and the other keys survive. Text
    /// that is not a JSON object reads as the default state. Use
    /// [`Settings::window_state_repaired`] to learn whether a repair happened.
    ///
    /// # Errors
    ///
    /// Returns [`SettingsError::Db`] when the read fails.
    pub fn window_state_value(&self) -> Result<Option<WindowState>, SettingsError> {
        Ok(self.window_state_repaired()?.map(|(state, _)| state))
    }

    /// Read the typed window state with read-repair, and report the repair.
    ///
    /// The flag is `true` when the stored text was not a valid window state.
    /// The stored text is not rewritten.
    ///
    /// # Errors
    ///
    /// Returns [`SettingsError::Db`] when the read fails.
    pub fn window_state_repaired(&self) -> Result<Option<(WindowState, bool)>, SettingsError> {
        let Some(text) = self.db.setting(WINDOW_STATE_KEY)? else {
            return Ok(None);
        };
        Ok(Some(repair(&text)))
    }

    /// Read the typed window state and refuse a malformed stored value.
    ///
    /// # Errors
    ///
    /// Returns [`DbError::WindowState`] inside [`SettingsError::Db`] when the
    /// stored text is not a window state. Returns [`SettingsError::Db`] when
    /// the read fails.
    pub fn window_state_strict(&self) -> Result<Option<WindowState>, SettingsError> {
        let Some(text) = self.db.setting(WINDOW_STATE_KEY)? else {
            return Ok(None);
        };
        let state = serde_json::from_str::<WindowState>(&text).map_err(DbError::from)?;
        Ok(Some(state))
    }

    /// Write a typed [`WindowState`] to the hidden `window_state` key.
    ///
    /// # Errors
    ///
    /// Returns [`SettingsError::InvalidWindowState`] when the value cannot
    /// encode. Returns [`SettingsError::Db`] when the write fails.
    pub fn set_window_state_value(&self, value: &WindowState) -> Result<(), SettingsError> {
        let text = serde_json::to_string(value).map_err(|_| SettingsError::InvalidWindowState {
            key: WINDOW_STATE_KEY,
        })?;
        self.db.set_setting(WINDOW_STATE_KEY, &text)?;
        Ok(())
    }
}

/// Parse stored text field by field and report whether a repair was needed.
///
/// A named key with the wrong type reads as `None`, so the valid keys and the
/// extra keys survive. Text that is not a JSON object reads as the default.
pub(super) fn repair(text: &str) -> (WindowState, bool) {
    if let Ok(state) = serde_json::from_str::<WindowState>(text) {
        return (state, false);
    }
    let Ok(Value::Object(mut map)) = serde_json::from_str::<Value>(text) else {
        return (WindowState::default(), true);
    };
    let mut field = |key: &str| map.remove(key).unwrap_or(Value::Null);
    let layout = serde_json::from_value(field("layout")).unwrap_or_default();
    let geometry = serde_json::from_value(field("geometry")).unwrap_or_default();
    let sort_mode = serde_json::from_value(field("sort_mode")).unwrap_or_default();
    let state = WindowState {
        layout,
        geometry,
        sort_mode,
        extra: map,
    };
    (state, true)
}
