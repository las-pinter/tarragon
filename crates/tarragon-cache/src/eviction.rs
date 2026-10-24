//! Cache size cap and LRU eviction.
//!
//! The cap is a total disk-size limit with a default of 20 GB. When the total
//! exceeds the cap, eviction deletes the three tier files of the
//! least-recently-accessed source first, until the total is at or under the
//! cap. A cap of zero bytes disables the cap.
//!
//! The cache crate cannot reach the database, so the caller supplies the
//! eviction order from `thumbnails.last_accessed`.

use std::path::Path;

use tarragon_core::tier::Tier;

use crate::CacheError;
use crate::store::{CacheEntry, CacheStore};

/// The result of one eviction pass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvictionOutcome {
    /// The sources whose tier files were removed, least recently accessed
    /// first.
    pub evicted: Vec<CacheEntry>,
    /// The total cache bytes after the pass.
    pub remaining_bytes: u64,
}

impl CacheStore {
    /// Return the total byte size of every regular file under the cache root.
    ///
    /// The total counts every file, including the `format=1` markers. A
    /// missing root gives zero.
    ///
    /// Leftover temp files count too. [`CacheStore::sweep_orphan_temps`]
    /// removes them at start.
    ///
    /// The walk intentionally skips symlinks. A tree with links can give two
    /// different totals. The skip avoids a link cycle and a double count.
    ///
    /// # Errors
    ///
    /// Returns [`CacheError::Io`] when a directory or file read fails.
    pub fn total_size(&self) -> Result<u64, CacheError> {
        total_size_under(self.root())
    }

    /// Evict least-recently-accessed sources until the total is at or under
    /// `cap_bytes`.
    ///
    /// `lru_order` holds the cache entries ordered least recently accessed
    /// first. The caller derives the order from `thumbnails.last_accessed`.
    /// A `cap_bytes` of zero disables the cap, so the pass evicts nothing. A
    /// source with a partial tier set loses only the files
    /// it has. A source with no tier files changes nothing and is not listed
    /// as evicted.
    ///
    /// The pass keeps a running total. It reads each tier file size before
    /// removal, subtracts the removed bytes, and does one final size walk to
    /// guard drift. So the pass is linear in the tree, not linear per source.
    /// The final walk is the reported `remaining_bytes`.
    ///
    /// When `lru_order` runs out while the total is still over the cap, the
    /// pass returns `Ok` with `remaining_bytes > cap_bytes`. The cap is best
    /// effort, not a hard promise.
    ///
    /// # Errors
    ///
    /// Returns [`CacheError::Io`] when a size read or a file remove fails. A
    /// failed remove still attempts every tier of the entry, so the on-disk
    /// state stays deterministic. The first remove error returns.
    pub fn evict_to_cap(
        &self,
        cap_bytes: u64,
        lru_order: &[CacheEntry],
    ) -> Result<EvictionOutcome, CacheError> {
        let (evicted, _running, final_total) = self.evict_pass(cap_bytes, lru_order)?;
        Ok(EvictionOutcome {
            evicted,
            remaining_bytes: final_total,
        })
    }

    /// Run one eviction pass and return the evicted entries, the running
    /// total, and the final size walk.
    ///
    /// The running total and the final walk agree unless a concurrent writer
    /// changed the tree. The final walk is the reported total. The two values
    /// are separate, so a test can prove the running total.
    fn evict_pass(
        &self,
        cap_bytes: u64,
        lru_order: &[CacheEntry],
    ) -> Result<(Vec<CacheEntry>, u64, u64), CacheError> {
        let mut remaining = self.total_size()?;
        let mut evicted = Vec::new();
        if cap_bytes == 0 {
            return Ok((evicted, remaining, remaining));
        }
        for entry in lru_order {
            if remaining <= cap_bytes {
                break;
            }
            let removed = remove_entry_files(entry, self.root())?;
            if removed.any {
                remaining = remaining.saturating_sub(removed.bytes);
                evicted.push(entry.clone());
            }
        }
        // One final walk guards drift from a concurrent atomic write or a
        // byte that the running total missed.
        let final_total = self.total_size()?;
        Ok((evicted, remaining, final_total))
    }
}

/// The byte total and progress of one entry removal.
#[derive(Debug, Default)]
struct Removed {
    /// The sum of the regular tier file sizes that were removed.
    bytes: u64,
    /// True when at least one tier file was removed.
    any: bool,
}

/// Remove the three tier files of `entry` under `root`.
///
/// A missing tier file is skipped, so a partial tier set is handled. A path
/// that is not under `root` is skipped as defense in depth. A directory or a
/// symlink is skipped, so the count matches the size walk. The helper reads
/// each tier file size before removal, so the caller keeps a running total.
///
/// The helper attempts every tier. A later failure does not stop an earlier
/// removal, so the on-disk state stays deterministic. It returns the first
/// remove error after the loop.
///
/// # Errors
///
/// Returns [`CacheError::Io`] when a remove fails for a reason other than a
/// missing file.
fn remove_entry_files(entry: &CacheEntry, root: &Path) -> Result<Removed, CacheError> {
    let canonical_root = std::fs::canonicalize(root).ok();
    let mut result = Removed::default();
    let mut first_error: Option<CacheError> = None;
    for tier in Tier::ALL {
        let path = entry.path(tier);
        if !is_within_root(canonical_root.as_deref(), path) {
            continue;
        }
        let size = match std::fs::symlink_metadata(path) {
            Ok(metadata) => {
                if !metadata.is_file() {
                    continue;
                }
                metadata.len()
            }
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => continue,
            Err(source) => {
                first_error.get_or_insert_with(|| CacheError::Io {
                    path: path.to_path_buf(),
                    source,
                });
                continue;
            }
        };
        match std::fs::remove_file(path) {
            Ok(()) => {
                result.any = true;
                result.bytes = result.bytes.saturating_add(size);
            }
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => {}
            Err(source) => {
                first_error.get_or_insert_with(|| CacheError::Io {
                    path: path.to_path_buf(),
                    source,
                });
            }
        }
    }
    first_error.map_or(Ok(result), Err)
}

/// Return true when the canonical `path` is under the canonical `root`.
///
/// A missing root or a missing path is not under the root. The check follows
/// symlinks, so a link that points outside the root is rejected.
pub(crate) fn is_within_root(canonical_root: Option<&Path>, path: &Path) -> bool {
    let Some(root) = canonical_root else {
        return false;
    };
    std::fs::canonicalize(path).is_ok_and(|canonical| canonical.starts_with(root))
}

/// Sum the regular file sizes under `root`.
///
/// The walk uses an explicit stack, so a deep tree cannot overflow the call
/// stack. A missing directory contributes zero. The walk never follows a
/// symlink, so a link cycle cannot loop it and a symlink is not counted. The
/// skip is intentional. A `NotFound` file is skipped, because a concurrent
/// atomic write can remove a file in the middle of the walk.
///
/// # Errors
///
/// Returns [`CacheError::Io`] when a directory or file read fails.
fn total_size_under(root: &Path) -> Result<u64, CacheError> {
    let mut total = 0u64;
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = match std::fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => continue,
            Err(source) => return Err(CacheError::Io { path: dir, source }),
        };
        for item in entries {
            let item = item.map_err(|source| CacheError::Io {
                path: dir.clone(),
                source,
            })?;
            let path = item.path();
            let file_type = item.file_type().map_err(|source| CacheError::Io {
                path: path.clone(),
                source,
            })?;
            if file_type.is_dir() {
                stack.push(path);
            } else if file_type.is_file() {
                let metadata = match item.metadata() {
                    Ok(metadata) => metadata,
                    Err(source) if source.kind() == std::io::ErrorKind::NotFound => continue,
                    Err(source) => return Err(CacheError::Io { path, source }),
                };
                total += metadata.len();
            }
        }
    }
    Ok(total)
}

#[cfg(test)]
mod tests;
