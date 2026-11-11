//! The grid keys: each one raises the event of the menu row with the same job.

use egui::{Key, Modifiers, Pos2};

use super::Gallery;
use super::Scope;
use super::rows::{Kind, event};

/// The keys, the modifiers they need, and the row each one stands for.
const BINDINGS: [(Key, Modifiers, Kind); 5] = [
    (Key::F2, Modifiers::NONE, Kind::Rename),
    (Key::Delete, Modifiers::NONE, Kind::Delete),
    (Key::Delete, Modifiers::SHIFT, Kind::DeletePermanently),
    (Key::C, Modifiers::COMMAND, Kind::Copy),
    (Key::V, Modifiers::COMMAND, Kind::Paste),
];

impl Gallery {
    /// The keys and modifiers that act like a menu row.
    pub(crate) fn grid_keys() -> impl Iterator<Item = (Key, Modifiers)> {
        BINDINGS.iter().map(|&(key, modifiers, _)| (key, modifiers))
    }

    /// Whether the press of a grid key would raise an event now.
    ///
    /// No key acts while a menu is open. Rename needs exactly one selected
    /// file, Paste needs one open folder, and every other key needs a selected
    /// file. A press of any other key gives false.
    pub(crate) fn grid_key_ready(&self, key: Key, modifiers: Modifiers) -> bool {
        let Some(kind) = bound_kind(key, modifiers) else {
            return false;
        };
        if self.tile_menu.open_at.is_some() {
            return false;
        }
        match kind {
            Kind::Paste => self.scope == Scope::Folder && self.folder.is_some(),
            Kind::Rename => self.selection.len() == 1,
            _ => !self.selection.is_empty(),
        }
    }

    /// Queue the event of a grid key, the same event its menu row gives. The
    /// caller checks [`Gallery::grid_key_ready`] first. A key with no row
    /// does nothing.
    pub(crate) fn raise_grid_key(&mut self, key: Key, modifiers: Modifiers) {
        let Some(kind) = bound_kind(key, modifiers) else {
            return;
        };
        if let Some(event) = event(kind, self.selected_paths(), Pos2::ZERO) {
            self.tile_menu.events.push(event);
        }
    }
}

/// The row that `key` held with `modifiers` stands for.
fn bound_kind(key: Key, modifiers: Modifiers) -> Option<Kind> {
    BINDINGS
        .iter()
        .find(|&&(bound, held, _)| bound == key && held == modifiers)
        .map(|&(.., kind)| kind)
}
