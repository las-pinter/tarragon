//! Cache size cap and LRU eviction (Q25, Q33).
//!
//! The cap is a total disk-size limit with a default of 20 GB. When the total
//! exceeds the cap, eviction deletes the three tier files of the
//! least-recently-accessed source first, until the total is at or under the
//! cap (Q25, Q33). A cap of zero bytes disables the cap.
//!
//! The cache crate cannot reach the database (Q16), so the caller supplies the
//! eviction order from `thumbnails.last_accessed` (Q21, Q33).

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
    /// The total counts every file, including the `format=1` markers, as the
    /// Python `compute_cache_size_bytes` does. A missing root gives zero.
    ///
    /// The Rust walk intentionally skips symlinks, unlike the Python `rglob`
    /// walk, which follows them. A tree with links can give two different
    /// totals. The Rust behavior avoids a link cycle and a double count.
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
    /// first. The caller derives the order from `thumbnails.last_accessed`
    /// (Q33). A `cap_bytes` of zero disables the cap, so the pass evicts
    /// nothing (Q33). A source with a partial tier set loses only the files
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
fn is_within_root(canonical_root: Option<&Path>, path: &Path) -> bool {
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
/// Python `rglob` walk follows links, so it can count a link target twice.
/// The Rust skip is intentional. A `NotFound` file is skipped, because a
/// concurrent atomic write can remove a file in the middle of the walk.
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
mod tests {
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};

    use tarragon_core::tier::Tier;

    use crate::store::{CacheEntry, CacheStore};

    /// A temporary cache root that removes itself on drop.
    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        /// Create a unique temp folder with the given tag.
        fn new(tag: &str) -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let id = NEXT.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir()
                .join(format!("tarragon-evict-{tag}-{}-{id}", std::process::id()));
            std::fs::create_dir_all(&path).expect("create the temp folder");
            Self { path }
        }

        /// Return the folder path.
        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }

    /// Write a tier file of `bytes` zero bytes.
    fn seed_tier(entry: &CacheEntry, tier: Tier, bytes: usize) {
        let path = entry.path(tier);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create the tier dir");
        }
        std::fs::write(path, vec![0u8; bytes]).expect("write the tier file");
    }

    /// Write every tier file of `entry` at one byte count.
    fn seed(entry: &CacheEntry, bytes: usize) {
        for tier in Tier::ALL {
            seed_tier(entry, tier, bytes);
        }
    }

    /// Build the three entries of a three-source cache, oldest first.
    ///
    /// The file names run `zeta`, `beta`, `alpha`, so an alphabetical order
    /// differs from the LRU order.
    fn three_sources(store: &CacheStore, bytes: usize) -> [CacheEntry; 3] {
        let old = store.entry("gallery", "old", "zeta.jpg");
        let mid = store.entry("gallery", "mid", "beta.jpg");
        let new = store.entry("gallery", "new", "alpha.jpg");
        for entry in [&old, &mid, &new] {
            seed(entry, bytes);
        }
        [old, mid, new]
    }

    /// Return true when any tier file of `entry` exists.
    fn any_file_exists(entry: &CacheEntry) -> bool {
        Tier::ALL.iter().any(|tier| entry.path(*tier).exists())
    }

    #[test]
    fn eviction_removes_least_recently_accessed_sources_first() {
        let temp = TempDir::new("lru-order");
        let store = CacheStore::new(temp.path());
        let [old, mid, new] = three_sources(&store, 100);

        // The total is 900. A cap of 300 forces the two oldest sources out.
        let outcome = store
            .evict_to_cap(300, &[old.clone(), mid.clone(), new.clone()])
            .expect("evict");

        assert_eq!(
            outcome.evicted,
            vec![old.clone(), mid.clone()],
            "the oldest source must go first"
        );
        assert!(!any_file_exists(&old), "the oldest source must be gone");
        assert!(!any_file_exists(&mid), "the middle source must be gone");
        assert!(any_file_exists(&new), "the newest source must stay");
    }

    #[test]
    fn eviction_stops_as_soon_as_the_total_is_under_the_cap() {
        let temp = TempDir::new("stop");
        let store = CacheStore::new(temp.path());
        let [old, mid, new] = three_sources(&store, 100);

        // After the two oldest sources the total is 300, below the 500 cap.
        let outcome = store
            .evict_to_cap(500, &[old, mid, new.clone()])
            .expect("evict");

        assert_eq!(outcome.evicted.len(), 2, "the pass must stop at the cap");
        assert_eq!(outcome.remaining_bytes, 300);
        assert!(
            outcome.remaining_bytes <= 500,
            "the pass must stop under the cap"
        );
        assert!(
            store.total_size().expect("total") == 300,
            "the on-disk total must match the outcome"
        );
        assert!(any_file_exists(&new), "the pass never touched the newest");
    }

    #[test]
    fn eviction_removes_all_three_tier_files_of_a_source() {
        let temp = TempDir::new("three-tiers");
        let store = CacheStore::new(temp.path());
        let [old, mid, new] = three_sources(&store, 100);

        store
            .evict_to_cap(300, &[old.clone(), mid, new])
            .expect("evict");

        for tier in Tier::ALL {
            let path = old.path(tier);
            assert!(
                !path.exists(),
                "tier {} must be removed: {}",
                tier.as_str(),
                path.display()
            );
        }
    }

    #[test]
    fn a_zero_cap_disables_eviction() {
        let temp = TempDir::new("zero-cap");
        let store = CacheStore::new(temp.path());
        let entries = three_sources(&store, 100);
        let total = store.total_size().expect("total");

        let outcome = store.evict_to_cap(0, &entries.clone()).expect("evict");

        assert!(outcome.evicted.is_empty(), "a zero cap evicts nothing");
        assert_eq!(outcome.remaining_bytes, total);
        for entry in &entries {
            assert!(any_file_exists(entry), "a zero cap must keep every file");
        }
    }

    #[test]
    fn an_under_cap_total_evicts_nothing() {
        let temp = TempDir::new("under-cap");
        let store = CacheStore::new(temp.path());
        let entries = three_sources(&store, 100);
        let total = store.total_size().expect("total");

        let outcome = store
            .evict_to_cap(total + 1, &entries.clone())
            .expect("evict");

        assert!(
            outcome.evicted.is_empty(),
            "an under-cap total evicts nothing"
        );
        assert_eq!(outcome.remaining_bytes, total);
        for entry in &entries {
            assert!(
                any_file_exists(entry),
                "an under-cap total keeps every file"
            );
        }
    }

    #[test]
    fn total_size_counts_a_partial_tier_set() {
        let temp = TempDir::new("partial");
        let store = CacheStore::new(temp.path());
        let full = store.entry("gallery", "full", "one.jpg");
        let partial = store.entry("gallery", "part", "two.jpg");
        seed(&full, 100);
        seed_tier(&partial, Tier::Thumb256, 50);
        seed_tier(&partial, Tier::Full, 25);

        assert_eq!(
            store.total_size().expect("total"),
            375,
            "the total must count only the files that exist"
        );
    }

    #[test]
    fn eviction_removes_a_partial_tier_set_and_skips_an_empty_source() {
        let temp = TempDir::new("partial-evict");
        let store = CacheStore::new(temp.path());
        let empty = store.entry("gallery", "empty", "none.jpg");
        let partial = store.entry("gallery", "part", "two.jpg");
        let full = store.entry("gallery", "full", "one.jpg");
        seed_tier(&partial, Tier::Preview1024, 100);
        seed(&full, 100);

        // The empty source changes nothing and must not be listed as evicted.
        let outcome = store
            .evict_to_cap(150, &[empty.clone(), partial.clone(), full.clone()])
            .expect("evict");

        assert_eq!(
            outcome.evicted,
            vec![partial.clone(), full.clone()],
            "an empty source is skipped and the pass continues"
        );
        assert_eq!(outcome.remaining_bytes, 0);
        assert!(
            !any_file_exists(&partial),
            "the partial source must be gone"
        );
        assert!(!any_file_exists(&full), "the full source must be gone");
    }

    #[test]
    fn remove_entry_files_reports_the_bytes_it_removed() {
        let temp = TempDir::new("removed-bytes");
        let store = CacheStore::new(temp.path());
        let entry = store.entry("gallery", "one", "one.jpg");
        seed(&entry, 100);
        let before = store.total_size().expect("total");

        let removed =
            super::remove_entry_files(&entry, store.root()).expect("remove the tier files");

        assert!(removed.any, "the helper must report a removal");
        assert_eq!(removed.bytes, 300, "the helper must report every tier byte");
        assert_eq!(
            store.total_size().expect("total"),
            before - removed.bytes,
            "the total must drop by exactly the removed bytes"
        );
    }

    #[test]
    fn evicting_exactly_two_sources_keeps_the_running_total() {
        let temp = TempDir::new("running-total");
        let store = CacheStore::new(temp.path());
        let [old, mid, new] = three_sources(&store, 100);
        let before = store.total_size().expect("total");

        // A cap of 300 forces exactly the two oldest sources out. The running
        // total after the pass is 300, and the final walk must agree.
        let (evicted, running, final_total) = store
            .evict_pass(300, &[old.clone(), mid.clone(), new.clone()])
            .expect("evict");

        assert_eq!(evicted, vec![old, mid], "two sources must go");
        assert_eq!(
            before - running,
            600,
            "the running total must drop by two 300-byte sources"
        );
        assert_eq!(
            running, final_total,
            "the running total must agree with the final walk"
        );
        assert_eq!(
            store.total_size().expect("total"),
            final_total,
            "the final walk must match the on-disk total"
        );
    }

    #[test]
    fn remove_entry_files_skips_a_path_outside_the_root() {
        let temp = TempDir::new("outside-root");
        let store = CacheStore::new(temp.path());
        let outside = TempDir::new("outside-target");
        let victim = outside.path().join("victim.jpg");
        std::fs::write(&victim, b"do not delete").expect("write the outside file");
        let entry = CacheEntry {
            thumb256: victim.clone(),
            preview1024: victim.clone(),
            full: victim.clone(),
        };

        let removed = super::remove_entry_files(&entry, store.root())
            .expect("the helper must skip the outside path");

        assert!(!removed.any, "a path outside the root must not be removed");
        assert_eq!(removed.bytes, 0, "an outside path removes no bytes");
        assert!(victim.is_file(), "the outside file must survive");
    }

    #[test]
    fn total_size_of_a_missing_root_is_zero() {
        let temp = TempDir::new("missing-root");
        let missing = temp.path().join("does-not-exist");
        let store = CacheStore::new(&missing);

        assert_eq!(store.total_size().expect("total"), 0);
    }

    #[cfg(unix)]
    #[test]
    fn total_size_does_not_count_a_symlink() {
        use std::os::unix::fs::symlink;

        let temp = TempDir::new("symlink");
        let store = CacheStore::new(temp.path());
        let entry = store.entry("gallery", "one", "one.jpg");
        seed(&entry, 100);
        let link = temp.path().join("link.jpg");
        symlink(entry.path(Tier::Thumb256), &link).expect("create the symlink");

        assert_eq!(
            store.total_size().expect("total"),
            300,
            "the walk must skip the symlink, unlike the Python rglob walk"
        );
    }

    #[test]
    fn an_exhausted_lru_list_returns_over_cap() {
        let temp = TempDir::new("exhausted");
        let store = CacheStore::new(temp.path());
        let [old, _mid, _new] = three_sources(&store, 100);
        let total = store.total_size().expect("total");

        let outcome = store
            .evict_to_cap(500, std::slice::from_ref(&old))
            .expect("evict");

        assert_eq!(outcome.evicted, vec![old], "only the listed source may go");
        assert_eq!(
            outcome.remaining_bytes,
            total - 300,
            "the pass must report the remaining over-cap total"
        );
        assert!(
            outcome.remaining_bytes > 500,
            "an exhausted list may leave the cache over the cap"
        );
    }
}
