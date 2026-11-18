//! Queueing an image for the app to open in an editor.

use std::path::PathBuf;

use egui::Id;

use super::Gallery;
use crate::grid::{CellClick, tile_id};

impl Gallery {
    /// Whether the widget `focused` is one of the tiles the last frame painted.
    #[must_use]
    pub(crate) fn is_tile(&self, focused: Id) -> bool {
        self.painted.iter().any(|&index| tile_id(index) == focused)
    }

    /// Return the queued image path once, then none.
    pub fn take_open_request(&mut self) -> Option<PathBuf> {
        self.open_request.take()
    }

    /// Queue the selected image when exactly one is selected.
    ///
    /// A newer request replaces one that the app has not taken yet.
    ///
    /// With none or several selected, the call does nothing.
    pub fn open_selected(&mut self) {
        if let Some(path) = self.sole_selected() {
            self.open_request = Some(path.to_path_buf());
        }
    }

    /// Select only the double-clicked tile and queue its image.
    ///
    /// The first click of the pair already ran, so the selection may hold more
    /// than one tile only when a modifier key was held. The call resets it, so
    /// one request never opens a second file.
    pub(super) fn apply_grid_double_click(&mut self, click: CellClick) {
        self.selection.select_only(click.index);
        self.sync_selection_revision();
        if let Some(item) = self.items.get(click.index) {
            self.open_request = Some(item.path.clone());
        }
    }
}
