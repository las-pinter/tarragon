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
        let path =
            std::env::temp_dir().join(format!("tarragon-evict-{tag}-{}-{id}", std::process::id()));
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
    let old = store
        .entry("gallery", "old", "zeta.jpg")
        .expect("valid names");
    let mid = store
        .entry("gallery", "mid", "beta.jpg")
        .expect("valid names");
    let new = store
        .entry("gallery", "new", "alpha.jpg")
        .expect("valid names");
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
    let full = store
        .entry("gallery", "full", "one.jpg")
        .expect("valid names");
    let partial = store
        .entry("gallery", "part", "two.jpg")
        .expect("valid names");
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
    let empty = store
        .entry("gallery", "empty", "none.jpg")
        .expect("valid names");
    let partial = store
        .entry("gallery", "part", "two.jpg")
        .expect("valid names");
    let full = store
        .entry("gallery", "full", "one.jpg")
        .expect("valid names");
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
    let entry = store
        .entry("gallery", "one", "one.jpg")
        .expect("valid names");
    seed(&entry, 100);
    let before = store.total_size().expect("total");

    let removed = super::remove_entry_files(&entry, store.root()).expect("remove the tier files");

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
    let entry = store
        .entry("gallery", "one", "one.jpg")
        .expect("valid names");
    seed(&entry, 100);
    let link = temp.path().join("link.jpg");
    symlink(entry.path(Tier::Thumb256), &link).expect("create the symlink");

    assert_eq!(
        store.total_size().expect("total"),
        300,
        "the walk must skip the symlink"
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
