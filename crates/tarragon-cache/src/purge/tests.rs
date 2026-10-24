use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use image::{DynamicImage, Rgba, RgbaImage};
use tarragon_core::tier::Tier;

use crate::CacheError;
use crate::store::CacheStore;

use super::PurgeOutcome;

/// A temporary folder that removes itself on drop.
struct TempDir {
    path: PathBuf,
}

impl TempDir {
    /// Create a unique temp folder with the given tag.
    fn new(tag: &str) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("tarragon-purge-{tag}-{}-{id}", std::process::id()));
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

/// Build a small deterministic source image.
fn source(width: u32, height: u32) -> DynamicImage {
    let rgba = RgbaImage::from_pixel(width, height, Rgba([10, 20, 30, 255]));
    DynamicImage::ImageRgba8(rgba)
}

/// Return the canonical form of `path`, which must exist.
fn canonical(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).expect("canonicalize the path")
}

#[test]
fn purge_removes_every_tier_file_and_marker() {
    let temp = TempDir::new("all");
    let store = CacheStore::new(temp.path());
    let entry = store
        .write_all(&source(40, 30), "gallery", "abcd1234", "art.jpg")
        .expect("write the tiers");
    assert!(store.total_size().expect("total") > 0);

    let outcome = store.purge(None).expect("purge");

    assert_eq!(
        outcome.files_removed, 6,
        "three tier files and three markers must go"
    );
    assert!(outcome.bytes_removed > 0, "the purge must count the bytes");
    assert_eq!(store.total_size().expect("total"), 0, "the tree must empty");
    assert!(temp.path().is_dir(), "the cache root itself must stay");
    for tier in Tier::ALL {
        assert!(
            !entry.path(tier).exists(),
            "tier {} must be gone",
            tier.as_str()
        );
        assert!(
            !store.marker_path(tier).exists(),
            "marker {} must be gone",
            tier.as_str()
        );
    }
}

#[test]
fn purge_of_a_missing_root_removes_nothing() {
    let temp = TempDir::new("missing");
    let store = CacheStore::new(temp.path().join("does-not-exist"));

    let outcome = store.purge(None).expect("purge a missing root");

    assert_eq!(outcome, PurgeOutcome::default());
}

#[cfg(unix)]
#[test]
fn purge_skips_a_link_that_points_outside_the_root() {
    use std::os::unix::fs::symlink;

    let temp = TempDir::new("escape");
    let store = CacheStore::new(temp.path());
    store
        .write_all(&source(40, 30), "gallery", "abcd1234", "art.jpg")
        .expect("write the tiers");
    let outside = TempDir::new("outside");
    let victim = outside.path().join("victim.jpg");
    std::fs::write(&victim, b"keep me").expect("write the victim");
    let link = temp.path().join("256").join("escape.jpg");
    symlink(&victim, &link).expect("make the escape link");

    store.purge(None).expect("purge");

    assert!(
        victim.is_file(),
        "the target outside the cache root must survive"
    );
    assert!(
        std::fs::symlink_metadata(&link).is_err(),
        "the link itself must be unlinked in place"
    );
}

#[cfg(unix)]
#[test]
fn purge_unlinks_a_broken_symlink() {
    use std::os::unix::fs::symlink;

    let temp = TempDir::new("broken-link");
    let store = CacheStore::new(temp.path());
    let tier = temp.path().join("256");
    std::fs::create_dir_all(&tier).expect("make the tier folder");
    let link = tier.join("broken.jpg");
    symlink(temp.path().join("does-not-exist.jpg"), &link).expect("make the broken link");

    store.purge(None).expect("purge");

    assert!(
        std::fs::symlink_metadata(&link).is_err(),
        "a broken link must be removed without an error"
    );
}

#[cfg(unix)]
#[test]
fn purge_unlinks_a_directory_symlink_to_an_outside_directory() {
    use std::os::unix::fs::symlink;

    let temp = TempDir::new("dir-link");
    let store = CacheStore::new(temp.path());
    let outside = TempDir::new("dir-link-outside");
    let victim_dir = outside.path().join("keep");
    std::fs::create_dir_all(&victim_dir).expect("make the outside directory");
    let victim_file = victim_dir.join("keep.jpg");
    std::fs::write(&victim_file, b"keep me").expect("write the outside file");
    let tier = temp.path().join("256");
    std::fs::create_dir_all(&tier).expect("make the tier folder");
    let link = tier.join("dir-link");
    symlink(&victim_dir, &link).expect("make the directory link");

    let outcome = store.purge(None).expect("purge");

    assert_eq!(
        outcome.files_removed, 1,
        "the directory link must count once"
    );
    assert!(
        victim_file.is_file(),
        "the target outside the cache root must survive"
    );
    assert!(
        std::fs::symlink_metadata(&link).is_err(),
        "the directory link itself must be unlinked in place"
    );
}

#[test]
fn is_within_root_accepts_only_a_path_inside_the_root() {
    let temp = TempDir::new("within");
    let inside_file = temp.path().join("256").join("art.jpg");
    std::fs::create_dir_all(inside_file.parent().expect("parent")).expect("make the dir");
    std::fs::write(&inside_file, b"inside").expect("write the inside file");
    let root = canonical(temp.path());
    let outside = TempDir::new("within-outside");
    let outside_file = outside.path().join("victim.jpg");
    std::fs::write(&outside_file, b"outside").expect("write the outside file");

    assert!(
        super::is_within_root(Some(&root), &inside_file),
        "an inside path must pass"
    );
    assert!(
        !super::is_within_root(Some(&root), &outside_file),
        "an outside path must fail"
    );
    assert!(
        !super::is_within_root(Some(&root), &temp.path().join("missing.jpg")),
        "a missing path must fail"
    );

    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        let link = temp.path().join("256").join("escape.jpg");
        symlink(&outside_file, &link).expect("make the escape link");
        assert!(
            !super::is_within_root(Some(&root), &link),
            "a symlink to an outside path must fail"
        );
    }
}

#[cfg(unix)]
#[test]
fn clear_tier_refuses_a_symlinked_tier_directory() {
    use std::os::unix::fs::symlink;

    let temp = TempDir::new("tier-symlink");
    let store = CacheStore::new(temp.path());
    let entry = store
        .write_all(&source(40, 30), "gallery", "abcd1234", "art.jpg")
        .expect("write the tiers");
    let before_256 = std::fs::read(&entry.thumb256).expect("read the 256 tier");
    // Replace the real `full` directory with a link to `256`.
    std::fs::remove_dir_all(temp.path().join("full")).expect("remove the real full dir");
    symlink(temp.path().join("256"), temp.path().join("full")).expect("make the tier link");

    let outcome = store.clear_tier(Tier::Full, None).expect("clear full");

    assert_eq!(
        outcome,
        PurgeOutcome::default(),
        "a symlinked tier must be refused"
    );
    assert_eq!(
        std::fs::read(&entry.thumb256).expect("read the 256 tier again"),
        before_256,
        "the 256 tier must stay byte-identical"
    );
    assert!(entry.thumb256.is_file(), "the 256 tier must survive");
}

#[test]
fn clear_tier_refuses_when_the_root_is_the_protected_path() {
    let temp = TempDir::new("clear-protect");
    let store = CacheStore::new(temp.path());
    store
        .write_all(&source(40, 30), "gallery", "abcd1234", "art.jpg")
        .expect("write the tiers");
    let root = canonical(temp.path());

    let error = store
        .clear_tier(Tier::Full, Some(&root))
        .expect_err("refuse the protected root");

    assert!(
        matches!(error, CacheError::UnsafeRoot(_)),
        "the error must name the unsafe root"
    );
    assert!(
        store.total_size().expect("total") > 0,
        "a refused clear must delete nothing"
    );
}

#[test]
fn the_filesystem_root_is_not_a_safe_purge_root() {
    assert!(!super::is_safe_purge_root(Path::new("/"), None));
}

#[test]
fn the_home_directory_is_not_a_safe_purge_root() {
    let home = TempDir::new("home");

    assert!(
        !super::is_safe_with_home(&canonical(home.path()), None, Some(home.path())),
        "the home directory must be refused"
    );
}

#[test]
fn an_ancestor_of_home_is_not_a_safe_purge_root() {
    let outer = TempDir::new("home-outer");
    let home = outer.path().join("home");
    std::fs::create_dir_all(&home).expect("make the home directory");

    assert!(
        !super::is_safe_with_home(&canonical(outer.path()), None, Some(&home)),
        "an ancestor of home must be refused"
    );
}

#[test]
fn purge_refuses_when_the_root_is_the_protected_path() {
    let temp = TempDir::new("protect-equal");
    let store = CacheStore::new(temp.path());
    store
        .write_all(&source(40, 30), "gallery", "abcd1234", "art.jpg")
        .expect("write the tiers");
    let root = canonical(temp.path());

    let error = store
        .purge(Some(&root))
        .expect_err("refuse the protected root");

    assert!(
        matches!(error, CacheError::UnsafeRoot(_)),
        "the error must name the unsafe root"
    );
    assert!(
        store.total_size().expect("total") > 0,
        "a refused purge must delete nothing"
    );
}

#[test]
fn purge_refuses_when_the_root_is_an_ancestor_of_the_protected_path() {
    let temp = TempDir::new("protect-ancestor");
    let store = CacheStore::new(temp.path());
    store
        .write_all(&source(40, 30), "gallery", "abcd1234", "art.jpg")
        .expect("write the tiers");
    // The parent exists because the root was canonicalized. The guard
    // compares the literal path components and does not strip the
    // filename.
    let protected = canonical(temp.path()).join("tarragon.db");

    let error = store
        .purge(Some(&protected))
        .expect_err("refuse the data-dir ancestor");

    assert!(
        matches!(error, CacheError::UnsafeRoot(_)),
        "the error must name the unsafe root"
    );
    assert!(
        store.total_size().expect("total") > 0,
        "a refused purge must delete nothing"
    );
}

#[test]
fn purge_allows_an_unrelated_protected_path() {
    let temp = TempDir::new("protect-unrelated");
    let store = CacheStore::new(temp.path());
    store
        .write_all(&source(40, 30), "gallery", "abcd1234", "art.jpg")
        .expect("write the tiers");
    let other = TempDir::new("protect-other");
    let protected = canonical(other.path());

    store
        .purge(Some(&protected))
        .expect("an unrelated protected path must pass");

    assert_eq!(
        store.total_size().expect("total"),
        0,
        "the purge must empty the tree"
    );
}

#[test]
fn clear_full_res_on_exit_true_removes_only_the_full_tier() {
    let temp = TempDir::new("clear-full");
    let store = CacheStore::new(temp.path());
    let entry = store
        .write_all(&source(40, 30), "gallery", "abcd1234", "art.jpg")
        .expect("write the tiers");
    let before_256 = std::fs::read(&entry.thumb256).expect("read the 256 tier");
    let before_1024 = std::fs::read(&entry.preview1024).expect("read the 1024 tier");

    let outcome = store
        .clear_full_res_on_exit(true, None)
        .expect("clear full");

    assert!(!entry.full.exists(), "the full tier file must be removed");
    assert!(
        !store.marker_path(Tier::Full).exists(),
        "the full tier marker must be removed"
    );
    for tier in [Tier::Thumb256, Tier::Preview1024] {
        assert!(
            entry.path(tier).is_file(),
            "tier {} must stay",
            tier.as_str()
        );
        assert!(
            store.marker_path(tier).is_file(),
            "marker {} must stay",
            tier.as_str()
        );
    }
    assert_eq!(
        std::fs::read(&entry.thumb256).expect("read the 256 tier again"),
        before_256,
        "the 256 tier must stay byte-identical"
    );
    assert_eq!(
        std::fs::read(&entry.preview1024).expect("read the 1024 tier again"),
        before_1024,
        "the 1024 tier must stay byte-identical"
    );
    assert_eq!(outcome.files_removed, 2, "one full file and one marker");
}

#[test]
fn clear_full_res_on_exit_false_removes_nothing() {
    let temp = TempDir::new("keep-full");
    let store = CacheStore::new(temp.path());
    let entry = store
        .write_all(&source(40, 30), "gallery", "abcd1234", "art.jpg")
        .expect("write the tiers");
    let before = store.total_size().expect("total");

    let outcome = store
        .clear_full_res_on_exit(false, None)
        .expect("clear full");

    assert_eq!(outcome, PurgeOutcome::default());
    assert_eq!(store.total_size().expect("total"), before);
    assert!(entry.full.is_file(), "the full tier file must stay");
}

/// Write a foreign file and a foreign folder beside the tiers.
fn add_foreign(root: &Path) -> (PathBuf, PathBuf) {
    let file = root.join("holiday.jpg");
    std::fs::write(&file, b"original").expect("write the foreign file");
    let folder = root.join("refs");
    std::fs::create_dir_all(&folder).expect("make the foreign folder");
    let nested = folder.join("sketch.png");
    std::fs::write(&nested, b"original").expect("write the nested file");
    (file, nested)
}

#[test]
fn purge_leaves_foreign_entries_in_the_root() {
    let temp = TempDir::new("foreign-purge");
    let store = CacheStore::new(temp.path());
    store
        .write_all(&source(40, 30), "gallery", "abcd1234", "art.jpg")
        .expect("write the tiers");
    let (file, nested) = add_foreign(temp.path());

    let outcome = store.purge(None).expect("purge");

    assert_eq!(outcome.files_removed, 6);
    assert!(file.is_file(), "a foreign file must survive the purge");
    assert!(nested.is_file(), "a foreign folder must survive the purge");
    assert!(!store.marker_path(Tier::Full).exists());
}

#[test]
fn clear_full_leaves_foreign_entries_in_the_root() {
    let temp = TempDir::new("foreign-full");
    let store = CacheStore::new(temp.path());
    let entry = store
        .write_all(&source(40, 30), "gallery", "abcd1234", "art.jpg")
        .expect("write the tiers");
    let (file, nested) = add_foreign(temp.path());

    store.clear_full_res_on_exit(true, None).expect("clear");

    assert!(file.is_file(), "a foreign file must survive the clear");
    assert!(nested.is_file(), "a foreign folder must survive the clear");
    assert!(!entry.full.exists());
    assert!(entry.thumb256.is_file(), "the other tiers stay");
}
