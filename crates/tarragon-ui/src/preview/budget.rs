//! Which previews fit the texture budget, at what size, and what to evict.
//!
//! The model holds keys and sizes only, so the loader can test it without a
//! GPU. [`TextureBudget`] does the byte count and the LRU order; this module
//! only picks the pixel size each image is uploaded at.

use std::collections::{HashMap, HashSet};
use std::hash::Hash;

use crate::texture_budget::{PREVIEW_BUDGET_BYTES, TextureBudget};

/// The longest side an upload may have. A full tier at 8192 px would take
/// 256 MiB alone, and many GPUs refuse a texture that wide.
pub const MAX_SIDE: u32 = 4096;

/// Bytes per pixel of an RGBA texture.
const BYTES_PER_PIXEL: usize = 4;

/// One requested image, with the size to upload it at.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Planned<K> {
    /// The image.
    pub key: K,
    /// The pixel size to upload at: the original, or scaled down to fit.
    pub size: (u32, u32),
    /// Whether a texture for it is already held. It is not decoded again.
    pub cached: bool,
}

/// What the loader should do for one selection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Plan<K> {
    /// One entry per requested image, in request order.
    pub items: Vec<Planned<K>>,
    /// The textures to drop, least recently used first.
    pub evict: Vec<K>,
}

/// The longest side one of `count` images may have so that all of them fit
/// in `capacity` bytes together. A square image is the worst case, so this
/// holds for any shape. It is at least 1 so that every image gets a plan.
#[must_use]
pub fn side_limit(count: usize, capacity: usize) -> u32 {
    let share = capacity / count.max(1) / BYTES_PER_PIXEL;
    u32::try_from(share.isqrt())
        .unwrap_or(MAX_SIDE)
        .clamp(1, MAX_SIDE)
}

/// `image` scaled down, aspect kept, until its longest side is at most
/// `limit`. A smaller image is kept as is. A side never drops below 1.
#[must_use]
pub fn scaled_size(image: (u32, u32), limit: u32) -> (u32, u32) {
    let longest = image.0.max(image.1);
    if longest <= limit {
        return image;
    }
    let scale = |side: u32| {
        let rounded =
            (u64::from(side) * u64::from(limit) + u64::from(longest) / 2) / u64::from(longest);
        u32::try_from(rounded).unwrap_or(limit).max(1)
    };
    (scale(image.0), scale(image.1))
}

/// The bytes of an RGBA texture of `size` pixels.
fn bytes_of(size: (u32, u32)) -> usize {
    size.0 as usize * size.1 as usize * BYTES_PER_PIXEL
}

/// Tracks the held preview textures and plans each new selection.
#[derive(Debug)]
pub struct PreviewTextures<K> {
    /// The byte count and the use order.
    budget: TextureBudget<K>,
    /// The pixel size each held texture was planned at. Two shapes can have
    /// the same byte count, so the bytes alone cannot tell a rotated or
    /// re-saved file from the held one. It is kept in step with `budget`.
    sizes: HashMap<K, (u32, u32)>,
}

impl<K: Hash + Eq + Clone> PreviewTextures<K> {
    /// Build an empty model with a budget of `capacity` bytes.
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        Self {
            budget: TextureBudget::new(capacity),
            sizes: HashMap::new(),
        }
    }

    /// Plan the images of a selection, given as keys with their pixel sizes.
    ///
    /// A key that appears twice is planned once, with its first size. A held
    /// texture counts as cached only when its held pixel size is the planned
    /// size; otherwise it is dropped from the budget and planned as new, and
    /// the loader uploads it again at that size. New images are recorded at
    /// their planned size.
    ///
    /// `evict` never lists a requested key. Normally the planned sizes make
    /// the whole selection fit, so nothing requested is pushed out. When the
    /// capacity is below 4 bytes per image the side limit is clamped to 1 and
    /// the selection cannot fit; the budget may then forget a requested key.
    /// That key is not listed in `evict`, and its item is marked not cached,
    /// so the loader replaces its texture by key. The loader may still hold
    /// that texture. If a later selection omits the key, nothing tells the
    /// loader to drop it.
    pub fn plan(&mut self, wanted: &[(K, (u32, u32))]) -> Plan<K> {
        let mut seen = HashSet::new();
        let wanted: Vec<_> = wanted
            .iter()
            .filter(|(key, _)| seen.insert(key.clone()))
            .collect();
        let limit = side_limit(wanted.len(), self.budget.capacity());
        let mut items = Vec::with_capacity(wanted.len());
        for (key, image) in wanted {
            let size = scaled_size(*image, limit);
            let held = self.sizes.get(key);
            let cached = held == Some(&size);
            if cached {
                self.budget.touch(key);
            } else if held.is_some() {
                self.remove(key);
            }
            items.push(Planned {
                key: key.clone(),
                size,
                cached,
            });
        }
        let mut evict = Vec::new();
        for item in items.iter().filter(|item| !item.cached) {
            evict.extend(self.budget.insert(item.key.clone(), bytes_of(item.size)));
            self.sizes.insert(item.key.clone(), item.size);
        }
        for key in &evict {
            self.sizes.remove(key);
        }
        evict.retain(|key| !seen.contains(key));
        for item in &mut items {
            if !self.budget.contains(&item.key) {
                self.sizes.remove(&item.key);
                item.cached = false;
            }
        }
        Plan { items, evict }
    }

    /// The bytes of all held textures.
    #[cfg(test)]
    fn budget_bytes(&self) -> usize {
        self.budget.total_bytes()
    }

    /// The size `key` is held at, when the budget tracks it.
    #[cfg(test)]
    fn held_size(&self, key: &K) -> Option<(u32, u32)> {
        self.budget
            .contains(key)
            .then(|| self.sizes.get(key).copied())
            .flatten()
    }

    /// Forget `key`, for example when its upload failed.
    pub fn remove(&mut self, key: &K) {
        self.budget.remove(key);
        self.sizes.remove(key);
    }
}

impl<K: Hash + Eq + Clone> Default for PreviewTextures<K> {
    fn default() -> Self {
        Self::new(PREVIEW_BUDGET_BYTES)
    }
}

#[cfg(test)]
mod tests;
