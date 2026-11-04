//! Tests for portable cache names in the store.

use super::{CacheStore, TempDir, source};
use crate::CacheError;
use crate::store::TierFormat;

/// Return the final file name of a path.
fn file_name(path: &std::path::Path) -> String {
    path.file_name()
        .and_then(|n| n.to_str())
        .expect("a utf-8 file name")
        .to_owned()
}

/// Return the directory name above a path.
fn dir_name(path: &std::path::Path) -> String {
    path.parent()
        .and_then(std::path::Path::file_name)
        .and_then(|n| n.to_str())
        .expect("a utf-8 directory name")
        .to_owned()
}

#[test]
fn illegal_characters_never_reach_the_disk_and_find_entry_agrees() {
    let temp = TempDir::new("names-illegal");
    let store = CacheStore::new(temp.path());
    let entry = store
        .write_all(&source(4, 4), "my:dir", "abcd", "a:b?.png")
        .expect("write");
    for path in [&entry.thumb256, &entry.preview1024, &entry.full] {
        assert!(path.is_file());
        let names = format!("{}{}", dir_name(path), file_name(path));
        assert!(!names.contains([':', '?']), "{names}");
    }
    let found = store
        .find_entry("my:dir", "abcd", "a:b?.png")
        .expect("probe")
        .expect("found");
    assert_eq!(found, entry);
}

#[test]
fn names_that_clean_alike_do_not_overwrite_each_other() {
    let temp = TempDir::new("names-collide");
    let store = CacheStore::new(temp.path());
    let first = store
        .write_all(&source(4, 4), "gallery", "abcd", "a:b.png")
        .expect("first");
    let second = store
        .write_all(&source(4, 4), "gallery", "abcd", "a_b.png")
        .expect("second");
    assert_ne!(first.full, second.full);
    assert!(first.all_files_exist());
    assert!(second.all_files_exist());
}

#[test]
fn a_reserved_device_name_gets_a_prefix_and_a_plain_name_is_unchanged() {
    let temp = TempDir::new("names-reserved");
    let store = CacheStore::new(temp.path());
    let con = store
        .entry("gallery", "abcd", "con.png", TierFormat::Jpeg)
        .expect("entry");
    assert!(file_name(&con.full).starts_with("_con.png-"));
    assert_eq!(con.full.extension().and_then(|e| e.to_str()), Some("jpg"));
    let art = store
        .entry("gallery", "abcd", "art.png", TierFormat::Jpeg)
        .expect("entry");
    assert_eq!(file_name(&art.full), "art.png.jpg");
    assert_eq!(dir_name(&art.full), "gallery_abcd");
}

#[test]
fn a_changed_name_keeps_the_payload_extension_last_and_sibling_keeps_the_hash() {
    let store = CacheStore::new("root");
    let png = store
        .entry("gallery", "abcd", "a:b.png", TierFormat::Png)
        .expect("entry");
    let jpeg = store
        .entry("gallery", "abcd", "a:b.png", TierFormat::Jpeg)
        .expect("entry");
    let name = file_name(&jpeg.full);
    assert!(name.starts_with("a_b.png-"));
    assert_eq!(jpeg.full.extension().and_then(|e| e.to_str()), Some("jpg"));
    assert_eq!(png.full.extension().and_then(|e| e.to_str()), Some("png"));
    assert_eq!(png.sibling(), jpeg);
    assert_eq!(jpeg.sibling(), png);
}

#[test]
fn a_separator_and_dot_dot_are_still_refused() {
    let store = CacheStore::new("root");
    for name in ["a/b", ".."] {
        assert!(matches!(
            store.entry("gallery", "abcd", name, TierFormat::Jpeg),
            Err(CacheError::InvalidName(_))
        ));
    }
    assert!(matches!(
        store.entry("a/b", "abcd", "art.png", TierFormat::Jpeg),
        Err(CacheError::InvalidName(_))
    ));
}

#[test]
fn entry_gives_equal_paths_on_every_call_for_a_changed_name() {
    let store = CacheStore::new("root");
    let first = store
        .entry("my:dir", "abcd", "a:b.png", TierFormat::Jpeg)
        .expect("entry");
    let second = store
        .entry("my:dir", "abcd", "a:b.png", TierFormat::Jpeg)
        .expect("entry");
    assert_eq!(first, second);
}

#[test]
fn tier_dir_cleans_an_illegal_folder_name() {
    let store = CacheStore::new("root");
    let dir = store.tier_dir(tarragon_core::tier::Tier::Full, "my:dir", "abcd");
    let name = dir
        .file_name()
        .and_then(|n| n.to_str())
        .expect("a utf-8 directory name");
    assert!(name.starts_with("my_dir-"), "{name}");
    assert!(name.ends_with("_abcd"), "{name}");
    assert!(!name.contains(':'), "{name}");
}

#[test]
fn a_changed_name_at_the_length_limit_gives_an_io_error_not_a_panic() {
    let temp = TempDir::new("names-limit");
    let store = CacheStore::new(temp.path());
    // 250 bytes fit as is, but the cleaned name gains 9 bytes.
    let name = format!(":{}", "n".repeat(249));
    let result = store.write_all(&source(4, 4), "gallery", "abcd", &name);
    assert!(matches!(result, Err(CacheError::Io { .. })));
}
