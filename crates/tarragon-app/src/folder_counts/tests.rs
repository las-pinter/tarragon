//! Tests for the folder counts.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use tarragon_ui::sidebar::panel::Sidebar;

use std::collections::BTreeSet;

use tarragon_core::error::ScanError;

use super::{FolderCounts, new_failures};

/// The longest a test waits for a count to arrive.
const WAIT_TIMEOUT: Duration = Duration::from_secs(10);

/// A temporary folder that removes itself on drop. The path is canonical, so
/// a watcher reports the same path on every platform.
struct TempDir {
    path: PathBuf,
}

impl TempDir {
    /// Create a unique temp folder with the given tag.
    fn new(tag: &str) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "tarragon-counts-{tag}-{}-{id}",
            tarragon_core::testing::run_tag()
        ));
        std::fs::create_dir_all(&path).expect("create the temp folder");
        let path = std::fs::canonicalize(&path).expect("resolve the temp folder");
        Self { path }
    }

    /// Make the subfolder `name` with `images` image files and return it.
    fn folder(&self, name: &str, images: usize) -> PathBuf {
        let path = self.path.join(name);
        std::fs::create_dir(&path).expect("make the folder");
        for index in 0..images {
            std::fs::write(path.join(format!("{index}.png")), b"x").expect("write");
        }
        path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// Counts with a wake counter.
fn counts() -> (FolderCounts, Arc<AtomicUsize>) {
    let wakes = Arc::new(AtomicUsize::new(0));
    let handle = Arc::clone(&wakes);
    let counts = FolderCounts::start(Arc::new(move || {
        handle.fetch_add(1, Ordering::SeqCst);
    }));
    (counts, wakes)
}

/// Apply results until `path` shows `want`, or the timeout runs out.
fn wait_for(counts: &FolderCounts, sidebar: &mut Sidebar, path: &Path, want: Option<u32>) -> bool {
    let deadline = Instant::now() + WAIT_TIMEOUT;
    while Instant::now() < deadline {
        counts.apply(sidebar);
        if sidebar.count(path) == want {
            return true;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    false
}

#[test]
fn a_shown_folder_is_counted_and_wakes_the_window() {
    let temp = TempDir::new("shown");
    let (a, b) = (temp.folder("a", 3), temp.folder("b", 0));
    let (counts, wakes) = counts();
    let mut sidebar = Sidebar::default();
    counts.set_visible(&[a.clone(), b.clone()]);
    assert!(wait_for(&counts, &mut sidebar, &a, Some(3)));
    assert_eq!(sidebar.count(&b), None, "zero shows nothing");
    assert!(wakes.load(Ordering::SeqCst) >= 1);
}

#[test]
fn a_new_image_in_a_shown_folder_updates_its_count() {
    let temp = TempDir::new("watch");
    let a = temp.folder("a", 1);
    let (counts, _) = counts();
    let mut sidebar = Sidebar::default();
    counts.set_visible(std::slice::from_ref(&a));
    assert!(wait_for(&counts, &mut sidebar, &a, Some(1)));
    std::fs::write(a.join("new.png"), b"x").expect("write");
    assert!(wait_for(&counts, &mut sidebar, &a, Some(2)), "no recount");
}

#[test]
fn a_removed_folder_loses_its_count() {
    let temp = TempDir::new("gone");
    let a = temp.folder("a", 2);
    let (counts, _) = counts();
    let mut sidebar = Sidebar::default();
    counts.set_visible(std::slice::from_ref(&a));
    assert!(wait_for(&counts, &mut sidebar, &a, Some(2)));
    std::fs::remove_dir_all(&a).expect("remove the folder");
    assert!(wait_for(&counts, &mut sidebar, &a, None), "the count stays");
}

#[test]
fn a_refused_folder_is_reported_once() {
    let refused = |path: &str| ScanError::Watch {
        path: PathBuf::from(path),
        reason: "gone".to_owned(),
    };
    let mut reported = BTreeSet::new();
    let first = [refused("/a"), refused("/b")];
    assert_eq!(new_failures(&first, &mut reported).len(), 2);
    let again = [refused("/a"), refused("/c")];
    let new = new_failures(&again, &mut reported);
    assert_eq!(new.len(), 1);
    assert!(matches!(new[0], ScanError::Watch { path, .. } if path == Path::new("/c")));
}

#[test]
fn a_folder_shown_again_is_counted_again() {
    let temp = TempDir::new("again");
    let a = temp.folder("a", 1);
    let (counts, _) = counts();
    let mut sidebar = Sidebar::default();
    counts.set_visible(std::slice::from_ref(&a));
    assert!(wait_for(&counts, &mut sidebar, &a, Some(1)));
    // While hidden, the folder is not watched, so its change goes unseen
    // until it shows again.
    counts.set_visible(&[]);
    std::fs::write(a.join("more.png"), b"x").expect("write");
    counts.set_visible(std::slice::from_ref(&a));
    assert!(wait_for(&counts, &mut sidebar, &a, Some(2)));
}
