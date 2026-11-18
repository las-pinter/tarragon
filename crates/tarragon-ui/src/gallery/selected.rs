//! Reading the selection from outside and noticing that it changed.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::Gallery;
use crate::grid::{CellClick, Selection};

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

    /// The path of the selected item when exactly one is selected. It copies
    /// nothing, so a caller can ask every frame.
    #[must_use]
    pub fn sole_selected(&self) -> Option<&Path> {
        if self.selection.len() != 1 {
            return None;
        }
        let index = self.selection.iter().next()?;
        self.items.get(index).map(|item| item.path.as_path())
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

    /// Select the visible tiles whose path is in `paths`, and scroll to the
    /// first of them on the next frame.
    ///
    /// Call it after `set_items` has loaded the new list, or it ignores the new
    /// paths. The anchor is the first listed path that is visible. A path that
    /// is not visible is ignored. When no path is visible the selection clears. The
    /// revision follows like it does for a click.
    pub fn select_paths(&mut self, paths: &[PathBuf]) {
        let position_of: HashMap<usize, usize> = self
            .visible
            .iter()
            .enumerate()
            .map(|(position, &index)| (index, position))
            .collect();
        let hits: Vec<(usize, usize)> = paths
            .iter()
            .filter_map(|path| {
                let index = *self.index.get(path)?;
                Some((index, *position_of.get(&index)?))
            })
            .collect();
        let first = hits.first().copied();
        self.selection = Selection::from_parts(
            hits.iter().map(|&(index, _)| index),
            first.map(|(index, _)| index),
        );
        self.scroll_to = first.map(|(_, position)| position);
        self.sync_selection_revision();
    }

    /// Record where the first of the `removed` paths stands in the display
    /// order.
    ///
    /// Call it before `set_items` drops the paths, then call
    /// [`Self::select_near`] after. A removed set that spans several tiles uses
    /// the first tile in display order. A set with no visible tile records
    /// nothing.
    pub fn remember_place(&mut self, removed: &[PathBuf]) {
        self.removed_at = self.visible.iter().position(|&index| {
            self.items
                .get(index)
                .is_some_and(|item| removed.contains(&item.path))
        });
    }

    /// Select the tile that took the place recorded by [`Self::remember_place`]
    /// and scroll to it on the next frame.
    ///
    /// Call it after `set_items` ran on a list without the removed paths. The
    /// tile at the recorded position is the successor of the removed tile. When
    /// the removed tile was last, the position is clamped, which picks the
    /// predecessor. With no tile left the selection clears. Without a recorded
    /// place the call does nothing. The place is used once.
    pub fn select_near(&mut self) {
        let Some(place) = self.removed_at.take() else {
            return;
        };
        if let Some(last) = self.visible.len().checked_sub(1) {
            let position = place.min(last);
            self.selection.select_only(self.visible[position]);
            self.scroll_to = Some(position);
        } else {
            self.selection.clear();
            self.scroll_to = None;
        }
        self.sync_selection_revision();
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
