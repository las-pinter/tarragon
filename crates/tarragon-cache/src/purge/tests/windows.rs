//! Purge tests that need Windows: a directory link is removed with
//! `remove_dir`, not `remove_file`.

use tarragon_core::tier::Tier;

use super::{TempDir, source};
use crate::store::CacheStore;
use crate::test_links::link_dir;

#[test]
fn purge_unlinks_a_directory_link_and_keeps_its_target() {
    let temp = TempDir::new("dirlink");
    let store = CacheStore::new(temp.path());
    store
        .write_all(&source(40, 30), "gallery", "abcd1234", "art.jpg")
        .expect("write the tiers");
    let outside = TempDir::new("dirlink-target");
    let kept = outside.path().join("keep.jpg");
    std::fs::write(&kept, b"keep me").expect("write the target file");
    let link = temp.path().join(Tier::Thumb320.as_str()).join("linked");
    if !link_dir(outside.path(), &link) {
        return;
    }

    store.purge(None).expect("purge");

    assert!(kept.is_file(), "the link target must survive");
    assert!(
        std::fs::symlink_metadata(&link).is_err(),
        "the directory link itself must be removed"
    );
}
