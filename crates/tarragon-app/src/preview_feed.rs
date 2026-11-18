//! The Preview textures that are held, and which ones a selection must load.
//! The plan has no egui calls. Only the `impl PreviewFeed<TextureHandle>`
//! block uploads textures, and it runs on a headless `Context` in tests.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use eframe::egui::{Context, TextureHandle, TextureOptions};
use tarragon_ui::preview::budget::PreviewTextures;
use tarragon_ui::preview::{Preview, PreviewImage};
use tarragon_ui::texture_budget::PREVIEW_BUDGET_BYTES;

use crate::preview_loader::{LoadItem, Loaded, PreviewLoader};
use crate::preview_source::Source;
use crate::thumbs::color_image;

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
    /// Whether the Preview must be given the held pictures again.
    dirty: bool,
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
            dirty: false,
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
        self.dirty = true;
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
        self.dirty = true;
    }
}

impl PreviewFeed<TextureHandle> {
    /// Upload the pictures the loader finished and show the selection in
    /// `preview`. `total` is the number of selected images. Returns whether
    /// the Preview was given pictures, so the caller can request a repaint.
    pub(crate) fn poll(
        &mut self,
        ctx: &Context,
        loader: &PreviewLoader,
        preview: &mut Preview,
        total: usize,
    ) -> bool {
        for loaded in loader.poll() {
            self.apply(ctx, loaded);
        }
        if !self.dirty {
            return false;
        }
        self.dirty = false;
        self.show(preview, total);
        true
    }

    /// Take one result. Returns whether a texture was stored. A result of the
    /// current request, good or failed, marks the Preview for a refresh.
    fn apply(&mut self, ctx: &Context, loaded: Loaded) -> bool {
        // A result of an older request must not upload or touch the plan.
        if loaded.revision != self.revision {
            return false;
        }
        let Some(&planned) = self.pending.get(&loaded.key) else {
            return false;
        };
        self.dirty = true;
        match loaded.image {
            Ok(image) => {
                let name = format!("preview:{}", loaded.key.display());
                let handle = ctx.load_texture(name, color_image(&image), TextureOptions::LINEAR);
                self.accept(&loaded.key, planned, handle)
            }
            Err(error) => {
                tracing::warn!("cannot show {}: {error}", loaded.key.display());
                // Left pending, every later select would decode the file again.
                self.pending.remove(&loaded.key);
                self.textures.remove(&loaded.key);
                false
            }
        }
    }

    /// Give `preview` the held pictures of the selection, in order.
    fn show(&self, preview: &mut Preview, total: usize) {
        let images: Vec<_> = self
            .wanted
            .iter()
            .filter_map(|key| self.held.get(key))
            .map(|handle| {
                let [width, height] = handle.size();
                PreviewImage {
                    texture: handle.id(),
                    size: (
                        u32::try_from(width).unwrap_or(u32::MAX),
                        u32::try_from(height).unwrap_or(u32::MAX),
                    ),
                }
            })
            .collect();
        let (image, mosaic, total) = route(images, total);
        preview.set_image(image);
        preview.set_mosaic(mosaic, total);
    }
}

/// Split the ready pictures into what the Preview takes: the single picture,
/// the mosaic pictures, and the selected count. One image goes to the single
/// picture, two or more to the mosaic.
fn route(
    images: Vec<PreviewImage>,
    total: usize,
) -> (Option<PreviewImage>, Vec<PreviewImage>, usize) {
    match total {
        0 => (None, Vec::new(), 0),
        1 => (images.first().copied(), Vec::new(), 1),
        _ => (None, images, total),
    }
}

#[cfg(test)]
mod poll_tests;
#[cfg(test)]
mod tests;
