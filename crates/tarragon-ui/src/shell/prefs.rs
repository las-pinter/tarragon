//! The Preferences window in the shell, and the menu choices that open it.

use super::Shell;
use crate::prefs::Prefs;

/// A menu choice that the app acts on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuEvent {
    /// The user chose Settings, Preferences. The app reads the settings and
    /// opens the window with `Shell::prefs_mut`.
    OpenPreferences,
}

impl Shell {
    /// Return the menu choices since the last call, and clear them.
    pub fn take_menu_events(&mut self) -> Vec<MenuEvent> {
        std::mem::take(&mut self.menu_events)
    }

    /// The Preferences window, for the app to open it and to read its events.
    pub fn prefs_mut(&mut self) -> &mut Prefs {
        &mut self.prefs
    }
}

#[cfg(test)]
mod tests;
