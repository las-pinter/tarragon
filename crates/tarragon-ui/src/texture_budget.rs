//! A byte budget for uploaded textures, with least-recently-used eviction.
//!
//! The type holds keys and sizes only. It has no egui types, so the app can
//! own the textures and call [`TextureBudget::insert`] to learn which ones to
//! drop.

use std::collections::{BTreeMap, HashMap};
use std::hash::Hash;

/// Grid thumbnail budget: 256 MiB.
pub const GRID_BUDGET_BYTES: usize = 256 * 1024 * 1024;
/// Preview budget: 128 MiB.
pub const PREVIEW_BUDGET_BYTES: usize = 128 * 1024 * 1024;

/// Tracks texture sizes and use order under a byte cap.
#[derive(Debug)]
pub struct TextureBudget<K> {
    /// The cap in bytes.
    capacity: usize,
    /// Sum of all tracked sizes.
    total: usize,
    /// Next use stamp. A higher stamp is a more recent use.
    clock: u64,
    /// Size and use stamp of each key.
    entries: HashMap<K, (usize, u64)>,
    /// Keys by use stamp, oldest first. This makes eviction O(log n).
    by_use: BTreeMap<u64, K>,
}

impl<K: Hash + Eq + Clone> TextureBudget<K> {
    /// Build an empty budget with a cap of `capacity` bytes.
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            total: 0,
            clock: 0,
            entries: HashMap::new(),
            by_use: BTreeMap::new(),
        }
    }

    /// The cap in bytes.
    #[must_use]
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Sum of all tracked sizes, in bytes.
    #[must_use]
    pub fn total_bytes(&self) -> usize {
        self.total
    }

    /// Number of tracked keys.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether no key is tracked.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Whether `key` is tracked.
    #[must_use]
    pub fn contains(&self, key: &K) -> bool {
        self.entries.contains_key(key)
    }

    /// Track `key` as the most recent use, with a size of `bytes`.
    ///
    /// Returns the keys to evict, least recently used first. The new key is
    /// never in the list. An item larger than the whole cap evicts every
    /// other key and stays alone, so the total may exceed the cap then. The
    /// caller still needs the one texture it just made.
    pub fn insert(&mut self, key: K, bytes: usize) -> Vec<K> {
        self.remove(&key);
        let stamp = self.next_stamp();
        self.entries.insert(key.clone(), (bytes, stamp));
        self.by_use.insert(stamp, key);
        self.total = self.total.saturating_add(bytes);

        let mut evicted = Vec::new();
        while self.total > self.capacity && self.entries.len() > 1 {
            let Some((_, oldest)) = self.by_use.pop_first() else {
                break;
            };
            if let Some((size, _)) = self.entries.remove(&oldest) {
                self.total -= size;
            }
            evicted.push(oldest);
        }
        evicted
    }

    /// Mark `key` as the most recent use. An unknown key is ignored.
    pub fn touch(&mut self, key: &K) {
        let stamp = self.next_stamp();
        let Some(entry) = self.entries.get_mut(key) else {
            return;
        };
        let old = std::mem::replace(&mut entry.1, stamp);
        self.by_use.remove(&old);
        self.by_use.insert(stamp, key.clone());
    }

    /// Stop tracking `key` and return its size, or none when unknown.
    pub fn remove(&mut self, key: &K) -> Option<usize> {
        let (bytes, stamp) = self.entries.remove(key)?;
        self.by_use.remove(&stamp);
        self.total -= bytes;
        Some(bytes)
    }

    /// The next use stamp.
    fn next_stamp(&mut self) -> u64 {
        self.clock += 1;
        self.clock
    }
}

#[cfg(test)]
mod tests;
