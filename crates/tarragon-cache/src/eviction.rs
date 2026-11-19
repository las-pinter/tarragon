//! Cache size cap and LRU eviction.
//!
//! The cap is a total disk-size limit with a default of 20 GiB (2^30 bytes per
//! GiB). The setting counts in GiB and this API counts in bytes, so the caller
//! converts. When the total exceeds the cap, eviction deletes the three tier
//! files of the least-recently-accessed source first, until the total is at or
//! under the cap. A cap of zero bytes disables the cap.
//!
//! The cache crate cannot reach the database, so the caller supplies the
//! eviction order from `thumbnails.last_accessed`.

use std::path::Path;

use tarragon_core::tier::Tier;

use crate::CacheError;
use crate::store::{CacheEntry, CacheStore};

/// The result of one eviction pass.
#[derive(Debug)]
pub struct EvictionOutcome {
    /// The sources whose tier files were removed, least recently accessed
    /// first. A source that lost only some of its files is listed too, so its
    /// row does not point at a removed file.
    pub evicted: Vec<CacheEntry>,
    /// The total cache bytes after the pass.
    pub remaining_bytes: u64,
    /// The first remove error, when a file could not be removed. The pass
    /// goes on with the next source after it.
    pub error: Option<CacheError>,
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
    /// A file that cannot be removed does not stop the pass: the first such
    /// error comes back in [`EvictionOutcome::error`], next to the sources
    /// that were removed.
    ///
    /// # Errors
    ///
    /// Returns [`CacheError::Io`] when a size walk fails.
    pub fn evict_to_cap(
        &self,
        cap_bytes: u64,
        lru_order: &[CacheEntry],
    ) -> Result<EvictionOutcome, CacheError> {
        let pass = self.evict_pass(cap_bytes, lru_order)?;
        Ok(EvictionOutcome {
            evicted: pass.evicted,
            remaining_bytes: pass.final_total,
            error: pass.error,
        })
    }

    /// Run one eviction pass.
    ///
    /// The running total and the final walk agree unless a concurrent writer
    /// changed the tree. The final walk is the reported total. The two values
    /// are separate, so a test can prove the running total.
    fn evict_pass(&self, cap_bytes: u64, lru_order: &[CacheEntry]) -> Result<Pass, CacheError> {
        let mut remaining = self.total_size()?;
        let mut pass = Pass {
            evicted: Vec::new(),
            running: remaining,
            final_total: remaining,
            error: None,
        };
        if cap_bytes == 0 {
            return Ok(pass);
        }
        for entry in lru_order {
            if remaining <= cap_bytes {
                break;
            }
            let (removed, error) = remove_entry_files(entry, self.root());
            if removed.any {
                remaining = remaining.saturating_sub(removed.bytes);
                pass.evicted.push(entry.clone());
            }
            if pass.error.is_none() {
                pass.error = error;
            }
        }
        pass.running = remaining;
        // One final walk guards drift from a concurrent atomic write or a
        // byte that the running total missed.
        pass.final_total = self.total_size()?;
        Ok(pass)
    }
}

/// What one eviction pass did.
#[derive(Debug)]
struct Pass {
    /// The sources that lost at least one file.
    evicted: Vec<CacheEntry>,
    /// The total the pass kept while it removed files.
    running: u64,
    /// The total of the final size walk.
    final_total: u64,
    /// The first remove error.
    error: Option<CacheError>,
}

/// The byte total and progress of one entry removal.
#[derive(Debug, Default)]
struct Removed {
    /// The sum of the regular tier file sizes that were removed.
    bytes: u64,
    /// True when at least one tier file was removed.
    any: bool,
}

/// Remove the three tier files of `entry` under `root`, in both formats.
///
/// A missing tier file is skipped, so a partial tier set is handled. A path
/// that is not under `root` is skipped as defense in depth. A directory or a
/// symlink is skipped, so the count matches the size walk. The helper reads
/// each tier file size before removal, so the caller keeps a running total.
///
/// The helper attempts every tier. A later failure does not stop an earlier
/// removal, so the on-disk state stays deterministic. Next to what it removed,
/// it returns the first error of a size read or a remove that failed for a
/// reason other than a missing file.
fn remove_entry_files(entry: &CacheEntry, root: &Path) -> (Removed, Option<CacheError>) {
    let canonical_root = dunce::canonicalize(root).ok();
    let mut result = Removed::default();
    let mut first_error: Option<CacheError> = None;
    let sibling = entry.sibling();
    for tier in Tier::ALL {
        // An earlier write may have used the other format. Both names go.
        for path in [entry.path(tier), sibling.path(tier)] {
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
    }
    (result, first_error)
}

/// Return true when the canonical `path` is under the canonical `root`.
///
/// A missing root or a missing path is not under the root. The check follows
/// symlinks, so a link that points outside the root is rejected.
pub(crate) fn is_within_root(canonical_root: Option<&Path>, path: &Path) -> bool {
    let Some(root) = canonical_root else {
        return false;
    };
    dunce::canonicalize(path).is_ok_and(|canonical| canonical.starts_with(root))
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
