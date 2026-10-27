//! Tests for the folder lister.

use super::*;
use std::sync::atomic::{AtomicU64, Ordering};

/// A unique temp folder that is removed on drop, even when a test fails.
struct TempDir(PathBuf);

impl TempDir {
    /// Remove any old folder of the same name, then create a fresh one.
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("tarragon-ui-lister-{}-{id}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create the temp dir");
        Self(dir)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The last component of each path, for short asserts.
fn names(paths: &[PathBuf]) -> Vec<String> {
    paths
        .iter()
        .map(|path| path.file_name().unwrap().to_string_lossy().into_owned())
        .collect()
}

#[test]
fn sorts_case_insensitively_with_exact_name_tie_break() {
    let temp = TempDir::new();
    for name in ["b", "B", "a", "Zed", "alpha"] {
        std::fs::create_dir(temp.0.join(name)).expect("create a folder");
    }
    let listed = list_subdirs(&temp.0);
    assert_eq!(names(&listed), ["a", "alpha", "B", "b", "Zed"]);
    assert!(listed.iter().all(|path| path.starts_with(&temp.0)));
}

#[test]
fn skips_dot_folders_and_files() {
    let temp = TempDir::new();
    std::fs::create_dir(temp.0.join(".hidden")).expect("create a folder");
    std::fs::create_dir(temp.0.join("shown")).expect("create a folder");
    std::fs::write(temp.0.join("file.png"), b"x").expect("create a file");
    assert_eq!(names(&list_subdirs(&temp.0)), ["shown"]);
}

#[cfg(unix)]
#[test]
fn shows_a_dir_symlink_and_skips_a_dangling_one() {
    let temp = TempDir::new();
    std::fs::create_dir(temp.0.join("real")).expect("create a folder");
    std::os::unix::fs::symlink(temp.0.join("real"), temp.0.join("link")).expect("link");
    std::os::unix::fs::symlink(temp.0.join("gone"), temp.0.join("dangling")).expect("link");
    assert_eq!(names(&list_subdirs(&temp.0)), ["link", "real"]);
}

#[test]
fn a_missing_folder_gives_an_empty_list() {
    let temp = TempDir::new();
    assert_eq!(list_subdirs(&temp.0.join("missing")), Vec::<PathBuf>::new());
}
