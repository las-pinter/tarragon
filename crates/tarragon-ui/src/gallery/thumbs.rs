//! Per-item thumbnail state and the visible-set report.
//!
//! The setters look an item up by its path and never touch the selection or
//! the sort. The UI crate holds textures only; the app decodes and uploads.

use std::path::Path;

use egui::TextureHandle;

use super::Gallery;
use crate::grid::TileError;

impl Gallery {
    /// True when the gallery has an item at `key`.
    #[must_use]
    pub fn has_item(&self, key: &Path) -> bool {
        self.index.contains_key(key)
    }

    /// Show `texture` on the item at `key`. Return false for an unknown key.
    ///
    /// A new texture also clears an earlier failure.
    pub fn set_texture(&mut self, key: &Path, texture: TextureHandle) -> bool {
        let Some(item) = self.item_mut(key) else {
            return false;
        };
        item.texture = Some(texture);
        item.error = None;
        true
    }

    /// Drop the texture of the item at `key`, so it shows the placeholder.
    /// Return false for an unknown key.
    pub fn clear_texture(&mut self, key: &Path) -> bool {
        let Some(item) = self.item_mut(key) else {
            return false;
        };
        item.texture = None;
        true
    }

    /// Mark the item at `key` as failed. Return false for an unknown key.
    ///
    /// The texture is dropped, so the tile shows the error mark.
    pub fn mark_failed(&mut self, key: &Path, error: TileError) -> bool {
        let Some(item) = self.item_mut(key) else {
            return false;
        };
        item.texture = None;
        item.error = Some(error);
        true
    }

    /// Paths of the tiles the last frame painted, in display order.
    ///
    /// The list is empty before the first frame.
    pub fn visible_keys(&self) -> impl Iterator<Item = &Path> {
        self.painted
            .iter()
            .filter_map(|&index| self.items.get(index))
            .map(|item| item.path.as_path())
    }

    /// A counter that grows each time the painted set changes.
    ///
    /// Compare it with the last value seen. An equal value means the same
    /// tiles, so the app skips the reprioritise call.
    #[must_use]
    pub fn visible_revision(&self) -> u64 {
        self.visible_revision
    }

    /// Record the tiles of this frame and bump the revision on a change.
    pub(super) fn record_painted(&mut self) {
        if self.painted_scratch == self.painted {
            return;
        }
        std::mem::swap(&mut self.painted, &mut self.painted_scratch);
        self.visible_revision += 1;
    }

    /// The item at `key`, found through the path index.
    fn item_mut(&mut self, key: &Path) -> Option<&mut crate::grid::GridItem> {
        let index = *self.index.get(key)?;
        self.items.get_mut(index)
    }
}
