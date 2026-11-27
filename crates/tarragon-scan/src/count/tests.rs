//! Tests for the folder count.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use super::count_supported;
use crate::scan::scan_folder;

/// A temporary folder that removes itself on drop.
struct TempDir {
    path: PathBuf,
}

impl TempDir {
    /// Create a unique temp folder with the given tag.
    fn new(tag: &str) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "tarragon-count-{tag}-{}-{id}",
            tarragon_core::testing::run_tag()
        ));
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

/// Write a small file at `folder/name`.
fn touch(folder: &Path, name: &str) {
    std::fs::write(folder.join(name), b"x").expect("write the file");
}

#[test]
fn the_count_matches_the_files_a_scan_keeps() {
    let temp = TempDir::new("match");
    for name in ["a.png", "b.JPG", "c.psd", "notes.txt", "d.kra", "noext"] {
        touch(temp.path(), name);
    }
    // A folder with an image name is not an image, and a subfolder's images
    // do not count.
    std::fs::create_dir(temp.path().join("folder.png")).expect("make the folder");
    std::fs::create_dir(temp.path().join("sub")).expect("make the subfolder");
    touch(&temp.path().join("sub"), "deep.png");
    let scanned = scan_folder(temp.path()).expect("scan the folder").len();
    assert_eq!(count_supported(temp.path()), Some(4));
    assert_eq!(
        Some(u32::try_from(scanned).unwrap()),
        count_supported(temp.path())
    );
}

#[test]
fn an_empty_folder_counts_zero() {
    let temp = TempDir::new("empty");
    assert_eq!(count_supported(temp.path()), Some(0));
}

#[test]
fn a_missing_folder_gives_no_count() {
    let temp = TempDir::new("missing");
    assert_eq!(count_supported(&temp.path().join("gone")), None);
}

#[test]
fn a_file_path_gives_no_count() {
    let temp = TempDir::new("file");
    touch(temp.path(), "a.png");
    assert_eq!(count_supported(&temp.path().join("a.png")), None);
}

#[cfg(unix)]
#[test]
fn a_link_counts_by_its_target() {
    let temp = TempDir::new("links");
    touch(temp.path(), "real.png");
    std::os::unix::fs::symlink(temp.path().join("real.png"), temp.path().join("link.png"))
        .expect("link a file");
    std::os::unix::fs::symlink(temp.path().join("gone.png"), temp.path().join("broken.png"))
        .expect("link a missing file");
    std::fs::create_dir(temp.path().join("dir")).expect("make the folder");
    std::os::unix::fs::symlink(temp.path().join("dir"), temp.path().join("dirlink.png"))
        .expect("link a folder");
    let scanned = scan_folder(temp.path()).expect("scan the folder").len();
    assert_eq!(count_supported(temp.path()), Some(2));
    assert_eq!(scanned, 2, "the scan keeps the same two");
}
