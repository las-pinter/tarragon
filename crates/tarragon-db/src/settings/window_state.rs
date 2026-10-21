//! The hidden window-state blob: layout, geometry, and sort mode.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tarragon_core::sorting::SortMode;

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
