//! Eviction tests that need Windows: a directory link under a tier folder.

use tarragon_core::tier::Tier;

use super::{TempDir, seed};
use crate::store::{CacheStore, TierFormat};
use crate::test_links::link_dir;

#[test]
fn eviction_neither_counts_nor_follows_a_directory_link() {
    let temp = TempDir::new("dirlink");
    let store = CacheStore::new(temp.path());
    let entry = store
        .entry("gallery", "one", "one.jpg", TierFormat::Jpeg)
        .expect("valid names");
    seed(&entry, 100);
    let outside = TempDir::new("dirlink-target");
    let kept = outside.path().join("keep.jpg");
    std::fs::write(&kept, vec![7u8; 1000]).expect("write the target file");
    let link = temp.path().join(Tier::Thumb320.as_str()).join("linked");
    if !link_dir(outside.path(), &link) {
        return;
    }

    assert_eq!(
        store.total_size().expect("total"),
        300,
        "the walk must not count the link target"
    );
    let outcome = store
        .evict_to_cap(1, std::slice::from_ref(&entry))
        .expect("evict");

    assert_eq!(outcome.evicted, vec![entry]);
    assert!(kept.is_file(), "the link target must survive");
    assert!(
        std::fs::symlink_metadata(&link).is_ok(),
        "eviction removes listed files only, so the link stays"
    );
}
