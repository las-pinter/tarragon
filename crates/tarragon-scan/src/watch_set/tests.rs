//! Tests for the folder set watcher.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::{Change, FolderSetWatcher, collect};
use crate::watch::DEBOUNCE;

/// The longest a test waits for a debounced callback.
const WAIT_TIMEOUT: Duration = Duration::from_secs(10);

/// A temporary folder that removes itself on drop.
///
/// The path is canonical, because some backends report events on the target
/// of a linked temp folder, such as `/private/tmp` on macOS.
struct TempDir {
    path: PathBuf,
}

impl TempDir {
    /// Create a unique temp folder with the given tag.
    fn new(tag: &str) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "tarragon-watchset-{tag}-{}-{id}",
            tarragon_core::testing::run_tag()
        ));
        std::fs::create_dir_all(&path).expect("create the temp folder");
        let path = std::fs::canonicalize(&path).expect("resolve the temp folder");
        Self { path }
    }

    /// Return the folder path.
    fn path(&self) -> &Path {
        &self.path
    }

    /// Make the subfolder `name` and return its path.
    fn sub(&self, name: &str) -> PathBuf {
        let path = self.path.join(name);
        std::fs::create_dir(&path).expect("make the subfolder");
        path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// A watcher whose callbacks land in a shared list.
fn recording() -> (FolderSetWatcher, Arc<Mutex<Vec<Change>>>) {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&seen);
    let watcher = FolderSetWatcher::start(move |change| {
        sink.lock().expect("the list").push(change);
    })
    .expect("start the watcher");
    (watcher, seen)
}

/// Wait until some change names `folder`, or the timeout runs out.
fn wait_for(seen: &Arc<Mutex<Vec<Change>>>, folder: &Path) -> bool {
    let deadline = Instant::now() + WAIT_TIMEOUT;
    while Instant::now() < deadline {
        if named(seen, folder) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    named(seen, folder)
}

/// Whether some change so far names `folder`.
fn named(seen: &Arc<Mutex<Vec<Change>>>, folder: &Path) -> bool {
    seen.lock()
        .expect("the list")
        .iter()
        .any(|change| match change {
            Change::Folders(folders) => folders.contains(folder),
            Change::All => false,
        })
}

/// Wait well past one window, so a late callback would have run.
fn settle() {
    std::thread::sleep(DEBOUNCE + Duration::from_millis(1500));
}

#[test]
fn a_new_file_names_its_watched_folder_once() {
    let temp = TempDir::new("create");
    let folder = temp.sub("a");
    let (mut watcher, seen) = recording();
    assert!(
        watcher
            .set_folders(std::slice::from_ref(&folder))
            .is_empty()
    );
    for index in 0..5 {
        std::fs::write(folder.join(format!("{index}.png")), b"x").expect("write");
    }
    assert!(wait_for(&seen, &folder), "no change named the folder");
    settle();
    assert_eq!(seen.lock().unwrap().len(), 1, "the burst gave one callback");
}

#[test]
fn a_folder_left_out_of_the_set_reports_nothing() {
    let temp = TempDir::new("unwatch");
    let (a, b) = (temp.sub("a"), temp.sub("b"));
    let (mut watcher, seen) = recording();
    assert!(watcher.set_folders(&[a.clone(), b.clone()]).is_empty());
    assert!(watcher.set_folders(std::slice::from_ref(&b)).is_empty());
    assert_eq!(watcher.folders(), &BTreeSet::from([b.clone()]));
    std::fs::write(a.join("x.png"), b"x").expect("write");
    std::fs::write(b.join("y.png"), b"x").expect("write");
    assert!(wait_for(&seen, &b), "the kept folder reports");
    settle();
    assert!(!named(&seen, &a), "the dropped folder still reports");
}

#[test]
fn a_missing_folder_is_refused_and_the_others_still_watch() {
    let temp = TempDir::new("missing");
    let a = temp.sub("a");
    let gone = temp.path().join("gone");
    let (mut watcher, seen) = recording();
    let errors = watcher.set_folders(&[a.clone(), gone.clone()]);
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(watcher.folders(), &BTreeSet::from([a.clone()]));
    std::fs::write(a.join("x.png"), b"x").expect("write");
    assert!(wait_for(&seen, &a));
}

#[test]
fn no_callback_runs_after_the_watcher_drops() {
    let temp = TempDir::new("drop");
    let a = temp.sub("a");
    let (mut watcher, seen) = recording();
    assert!(watcher.set_folders(std::slice::from_ref(&a)).is_empty());
    drop(watcher);
    std::fs::write(a.join("x.png"), b"x").expect("write");
    settle();
    assert!(seen.lock().unwrap().is_empty());
}

#[test]
fn an_event_names_the_path_and_its_parent() {
    let mut into = BTreeSet::new();
    collect(
        vec![PathBuf::from("/r/a/x.png"), PathBuf::from("/r/b")],
        &mut into,
    );
    let want: BTreeSet<PathBuf> = ["/r/a", "/r/a/x.png", "/r", "/r/b"]
        .into_iter()
        .map(PathBuf::from)
        .collect();
    assert_eq!(into, want);
}
