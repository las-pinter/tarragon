//! Reading the selection from outside and noticing that it changed.

use std::path::PathBuf;

use super::Gallery;
use crate::grid::CellClick;

impl Gallery {
    /// The paths of the selected items, in the order the grid shows them.
    ///
    /// The selection stores item indices in index order, so the display order
    /// comes from the cached visible list. Items hidden by a filter are not
    /// selected, so every selected index appears in it.
    #[must_use]
    pub fn selected_paths(&self) -> Vec<PathBuf> {
        self.visible
            .iter()
            .filter(|&&index| self.selection.contains(index))
            .map(|&index| self.items[index].path.clone())
            .collect()
    }

    /// A counter that grows each time `selected_paths` returns another list.
    ///
    /// A caller compares it with the last value it saw, which is cheaper than
    /// comparing the paths every frame. The order of the paths is part of the
    /// list, so a sort change that reorders two or more selected items grows
    /// it, and a click on the sole selected item does not.
    #[must_use]
    pub fn selection_revision(&self) -> u64 {
        self.selection_revision
    }

    /// Apply a grid cell click to the selection and update the revision.
    pub(super) fn apply_grid_click(&mut self, click: CellClick) {
        self.selection
            .apply_click(click.position, &self.visible, click.modifiers);
        self.sync_selection_revision();
    }

    /// Grow the revision if the ordered selected paths differ from the last
    /// ones seen.
    ///
    /// Every change of the selection or of the view ends in this call, so one
    /// rule covers clicks, filters, folders, scopes, sorts and new items. The
    /// paths are compared by reference and copied only on a change.
    pub(super) fn sync_selection_revision(&mut self) {
        let same = self
            .visible
            .iter()
            .filter(|&&index| self.selection.contains(index))
            .map(|&index| &self.items[index].path)
            .eq(self.seen_paths.iter());
        if !same {
            self.seen_paths = self.selected_paths();
            self.selection_revision += 1;
        }
    }
}
