//! The Preview textures that are held, and which ones a selection must load.
//! It has no egui calls, so tests run it without a window.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use tarragon_ui::preview::budget::PreviewTextures;
use tarragon_ui::texture_budget::PREVIEW_BUDGET_BYTES;

use crate::preview_loader::{LoadItem, PreviewLoader};
use crate::preview_source::Source;

/// The held Preview textures and the plan for the current selection.
/// `T` is the texture handle.
pub(crate) struct PreviewFeed<T> {
    /// The byte count, the use order, and the planned sizes.
    textures: PreviewTextures<PathBuf>,
    /// The handle of each held texture.
    held: HashMap<PathBuf, T>,
    /// Requested pictures that have no handle yet, with their planned size.
    /// The model counts a picture as cached when it is planned, before its
    /// texture exists, so this set tells a loaded picture from a requested one.
    pending: HashMap<PathBuf, (u32, u32)>,
    /// The revision of the latest request. A result of another revision is stale.
    revision: u64,
    /// The keys of the current selection, in display order.
    wanted: Vec<PathBuf>,
    /// The byte budget, kept to build a fresh model in `forget_all`.
    capacity: usize,
}

impl<T> PreviewFeed<T> {
    /// An empty feed with the Preview texture budget.
    pub(crate) fn new() -> Self {
        Self::with_capacity(PREVIEW_BUDGET_BYTES)
    }

    /// An empty feed with a budget of `capacity` bytes.
    fn with_capacity(capacity: usize) -> Self {
        Self {
            textures: PreviewTextures::new(capacity),
            held: HashMap::new(),
            pending: HashMap::new(),
            revision: 0,
            wanted: Vec::new(),
            capacity,
        }
    }

    /// Plan the selection in `source` and ask `loader` for the pictures that
    /// have no texture at their planned size, or have one still on its way.
    /// A picture with a handle keeps it. A handle is dropped when its key is
    /// evicted, or when its key is requested again at another size or the
    /// budget forgot it. The revision grows only when a request is sent.
    pub(crate) fn select(&mut self, source: &Source, loader: &PreviewLoader) {
        let sizes: Vec<_> = source
            .items
            .iter()
            .map(|item| (item.key.clone(), item.size))
            .collect();
        let plan = self.textures.plan(&sizes);
        for key in &plan.evict {
            self.held.remove(key);
        }
        let selected: HashSet<_> = plan.items.iter().map(|planned| &planned.key).collect();
        // The model counts a pending picture as held. One that left the
        // selection never gets a handle, so the model must forget it.
        let textures = &mut self.textures;
        self.pending.retain(|key, _| {
            let keep = selected.contains(key);
            if !keep {
                textures.remove(key);
            }
            keep
        });
        // `plan` lists a repeated key once, with its first size.
        let mut tiers = HashMap::new();
        for item in &source.items {
            tiers.entry(&item.key).or_insert(&item.tier);
        }
        let mut requests = Vec::new();
        for planned in &plan.items {
            let Some(tier) = tiers.get(&planned.key) else {
                continue;
            };
            if planned.cached && !self.pending.contains_key(&planned.key) {
                continue;
            }
            // The model dropped the old texture, so its handle must go too.
            self.held.remove(&planned.key);
            self.pending.insert(planned.key.clone(), planned.size);
            requests.push(LoadItem {
                key: planned.key.clone(),
                tier: (*tier).clone(),
                size: planned.size,
            });
        }
        self.wanted = plan.items.into_iter().map(|planned| planned.key).collect();
        if !requests.is_empty() {
            self.revision += 1;
            loader.request(self.revision, requests);
        }
    }

    /// Store the handle of a loaded picture. It is refused, and the handle
    /// dropped, unless `key` is still pending at `size`. Returns whether it was stored.
    pub(crate) fn accept(&mut self, key: &Path, size: (u32, u32), handle: T) -> bool {
        if self.pending.get(key) != Some(&size) {
            return false;
        }
        self.pending.remove(key);
        self.held.insert(key.to_path_buf(), handle);
        true
    }

    /// Drop every handle and the plan, for an empty selection.
    pub(crate) fn forget_all(&mut self) {
        self.textures = PreviewTextures::new(self.capacity);
        self.held.clear();
        self.pending.clear();
        self.wanted.clear();
    }
}

#[cfg(test)]
mod tests;
