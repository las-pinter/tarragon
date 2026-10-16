//! A live folder watcher with a 500 ms debounce (Q26).
//!
//! The watcher runs only while a folder is open (Q26). One burst of
//! filesystem events gives one rescan callback after [`DEBOUNCE`]. The watch
//! is non-recursive: only direct children of the folder report events.
//!
//! The backend `notify` handler sends a small message to a debounce thread.
//! The thread waits out the quiet window, then calls the rescan callback.
//! Stopping the watcher drops the backend and joins the thread, so a stale
//! rescan cannot escape after the folder closes (Q19).

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use tarragon_core::error::ScanError;

/// The debounce window: one burst of events gives one rescan after 500 ms (Q26).
pub const DEBOUNCE: Duration = Duration::from_millis(500);

/// A running watcher for one open folder.
///
/// Drop the value, or call [`FolderWatcher::stop`], to end the watch. The
/// backend drops first, then the debounce thread is joined.
pub struct FolderWatcher {
    folder: PathBuf,
    backend: Option<RecommendedWatcher>,
    debounce: Option<JoinHandle<()>>,
}

impl FolderWatcher {
    /// Start a non-recursive watch on `folder` (Q26).
    ///
    /// `on_rescan` runs on the debounce thread after each quiet window. The
    /// callback must not block for long, because it holds up the next window.
    ///
    /// # Errors
    ///
    /// Returns [`ScanError::Watch`] when the backend cannot watch the folder
    /// or the debounce thread cannot start.
    pub fn start<F>(folder: &Path, on_rescan: F) -> Result<Self, ScanError>
    where
        F: FnMut() + Send + 'static,
    {
        let (sender, receiver) = mpsc::channel::<()>();
        let mut backend =
            notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
                let Ok(event) = event else {
                    return;
                };
                if is_change(event.kind) {
                    // A send fault means the debounce thread already stopped.
                    let _ = sender.send(());
                }
            })
            .map_err(|error| watch_error(folder, &error))?;
        backend
            .watch(folder, RecursiveMode::NonRecursive)
            .map_err(|error| watch_error(folder, &error))?;
        let debounce = thread::Builder::new()
            .name("tarragon-watch".to_owned())
            .spawn(move || debounce_loop(&receiver, on_rescan))
            .map_err(|error| watch_error(folder, &error))?;
        Ok(Self {
            folder: folder.to_path_buf(),
            backend: Some(backend),
            debounce: Some(debounce),
        })
    }

    /// The watched folder.
    #[must_use]
    pub fn folder(&self) -> &Path {
        &self.folder
    }

    /// Return true while the backend watch is live.
    #[must_use]
    pub fn is_running(&self) -> bool {
        self.backend.is_some()
    }

    /// Stop the watch. Safe to call more than once.
    ///
    /// The backend drops first, so no new event reaches the thread. The call
    /// then joins the thread and drops a pending debounce.
    pub fn stop(&mut self) {
        self.backend = None;
        if let Some(handle) = self.debounce.take() {
            // A panicking callback is swallowed here; the watch still stops.
            let _ = handle.join();
        }
    }
}

impl Drop for FolderWatcher {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Wrap a backend or thread fault as a scan error.
fn watch_error(folder: &Path, reason: impl std::fmt::Display) -> ScanError {
    ScanError::Watch {
        path: folder.to_path_buf(),
        reason: reason.to_string(),
    }
}

/// Return true when the event kind can change the folder contents.
///
/// Only access events are dropped, because our own rescan reads the folder.
/// Keep `Other` and `Any`, because they can carry the rescan-on-drop signal.
fn is_change(kind: EventKind) -> bool {
    !matches!(kind, EventKind::Access(_))
}

/// Coalesce events and call `on_rescan` once per quiet window.
///
/// A disconnected channel means the backend dropped, so the thread returns
/// without calling `on_rescan`.
fn debounce_loop<F: FnMut()>(receiver: &Receiver<()>, mut on_rescan: F) {
    loop {
        // Block for the first event of a burst.
        if receiver.recv().is_err() {
            return;
        }
        // Wait out the quiet window. Each new event restarts the window.
        loop {
            match receiver.recv_timeout(DEBOUNCE) {
                Ok(()) => {}
                Err(RecvTimeoutError::Timeout) => break,
                Err(RecvTimeoutError::Disconnected) => return,
            }
        }
        on_rescan();
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
    use std::time::{Duration, Instant};

    use super::{DEBOUNCE, FolderWatcher, is_change};
    use crate::scan::scan_folder;
    use notify::EventKind;
    use notify::event::{AccessKind, CreateKind, ModifyKind, RemoveKind};

    /// The longest a test waits for a debounced callback.
    const WAIT_TIMEOUT: Duration = Duration::from_secs(10);

    /// Extra quiet time for a negative assert on a slow CI machine.
    const NEGATIVE_MARGIN: Duration = Duration::from_millis(1500);

    /// A short wait inside the debounce window, so work is still pending.
    const PARTIAL_WINDOW: Duration = Duration::from_millis(100);

    /// A temporary folder that removes itself on drop.
    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        /// Create a unique temp folder with the given tag.
        fn new(tag: &str) -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let id = NEXT.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir()
                .join(format!("tarragon-watch-{tag}-{}-{id}", std::process::id()));
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

    /// Build a callback that counts its calls.
    fn counter() -> (Arc<AtomicUsize>, impl FnMut() + Send + 'static) {
        let count = Arc::new(AtomicUsize::new(0));
        let handle = Arc::clone(&count);
        let callback = move || {
            handle.fetch_add(1, Ordering::SeqCst);
        };
        (count, callback)
    }

    /// Wait until the counter reaches `target`, or the timeout runs out.
    fn wait_for(count: &Arc<AtomicUsize>, target: usize) -> bool {
        let deadline = Instant::now() + WAIT_TIMEOUT;
        while Instant::now() < deadline {
            if count.load(Ordering::SeqCst) >= target {
                return true;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        count.load(Ordering::SeqCst) >= target
    }

    /// Wait one debounce window plus a margin, to catch a late callback.
    fn settle() {
        std::thread::sleep(DEBOUNCE + Duration::from_millis(300));
    }

    /// Wait well past the window, so a slow machine cannot fake a quiet watch.
    fn settle_long() {
        std::thread::sleep(DEBOUNCE + NEGATIVE_MARGIN);
    }

    #[test]
    fn one_create_triggers_one_rescan() {
        let temp = TempDir::new("create");
        let (count, callback) = counter();
        let _watcher = FolderWatcher::start(temp.path(), callback).expect("start the watch");
        std::fs::write(temp.path().join("new.png"), b"x").expect("write the file");
        assert!(wait_for(&count, 1), "no rescan after a create");
        settle();
        assert_eq!(
            count.load(Ordering::SeqCst),
            1,
            "the create gave extra rescans"
        );
    }

    #[test]
    fn one_modify_triggers_one_rescan() {
        let temp = TempDir::new("modify");
        let file = temp.path().join("art.png");
        std::fs::write(&file, b"x").expect("write the file");
        let (count, callback) = counter();
        let _watcher = FolderWatcher::start(temp.path(), callback).expect("start the watch");
        std::fs::write(&file, b"changed").expect("modify the file");
        assert!(wait_for(&count, 1), "no rescan after a modify");
        settle();
        assert_eq!(
            count.load(Ordering::SeqCst),
            1,
            "the modify gave extra rescans"
        );
    }

    #[test]
    fn one_delete_triggers_one_rescan() {
        let temp = TempDir::new("delete");
        let file = temp.path().join("art.png");
        std::fs::write(&file, b"x").expect("write the file");
        let (count, callback) = counter();
        let _watcher = FolderWatcher::start(temp.path(), callback).expect("start the watch");
        std::fs::remove_file(&file).expect("delete the file");
        assert!(wait_for(&count, 1), "no rescan after a delete");
        settle();
        assert_eq!(
            count.load(Ordering::SeqCst),
            1,
            "the delete gave extra rescans"
        );
    }

    #[test]
    fn a_burst_within_the_window_gives_one_rescan() {
        let temp = TempDir::new("burst");
        let (count, callback) = counter();
        let _watcher = FolderWatcher::start(temp.path(), callback).expect("start the watch");
        for index in 0..20 {
            std::fs::write(temp.path().join(format!("burst{index}.png")), b"x").expect("write");
        }
        assert!(wait_for(&count, 1), "no rescan after the burst");
        settle();
        assert_eq!(
            count.load(Ordering::SeqCst),
            1,
            "the burst gave more than one rescan"
        );
    }

    #[test]
    fn a_rescan_that_reads_the_folder_does_not_self_trigger() {
        let temp = TempDir::new("selftrigger");
        let folder = temp.path().to_path_buf();
        let count = Arc::new(AtomicUsize::new(0));
        let handle = Arc::clone(&count);
        let callback = move || {
            // A real rescan reads the folder; that read must not trigger again.
            let _ = scan_folder(&folder);
            handle.fetch_add(1, Ordering::SeqCst);
        };
        let _watcher = FolderWatcher::start(temp.path(), callback).expect("start the watch");
        std::fs::write(temp.path().join("new.png"), b"x").expect("write the file");
        assert!(wait_for(&count, 1), "no rescan after a create");
        settle_long();
        assert_eq!(
            count.load(Ordering::SeqCst),
            1,
            "the rescan retriggered itself"
        );
    }

    #[test]
    fn two_writes_past_the_window_give_two_rescans() {
        let temp = TempDir::new("twoburst");
        let (count, callback) = counter();
        let _watcher = FolderWatcher::start(temp.path(), callback).expect("start the watch");
        std::fs::write(temp.path().join("first.png"), b"x").expect("write the first file");
        assert!(wait_for(&count, 1), "no rescan after the first write");
        settle();
        std::fs::write(temp.path().join("second.png"), b"x").expect("write the second file");
        assert!(wait_for(&count, 2), "the watch stopped after one burst");
    }

    #[test]
    fn stop_ends_the_watch_and_drops_pending_work() {
        let temp = TempDir::new("stop");
        let (count, callback) = counter();
        let mut watcher = FolderWatcher::start(temp.path(), callback).expect("start the watch");
        assert!(watcher.is_running(), "the watch did not start");
        // Write inside the window, wait a little, then stop before it fires.
        std::fs::write(temp.path().join("pending.png"), b"x").expect("write the file");
        std::thread::sleep(PARTIAL_WINDOW);
        watcher.stop();
        assert!(
            !watcher.is_running(),
            "the watch is still running after stop"
        );
        settle_long();
        assert_eq!(
            count.load(Ordering::SeqCst),
            0,
            "a stopped watch called back"
        );
    }

    #[test]
    fn a_nested_change_is_ignored() {
        let temp = TempDir::new("nested");
        let nested = temp.path().join("sub");
        std::fs::create_dir_all(&nested).expect("make the subfolder");
        let file = nested.join("art.png");
        std::fs::write(&file, b"x").expect("write the nested file");
        let (count, callback) = counter();
        let _watcher = FolderWatcher::start(temp.path(), callback).expect("start the watch");
        std::fs::write(&file, b"changed").expect("modify the nested file");
        settle_long();
        assert_eq!(
            count.load(Ordering::SeqCst),
            0,
            "the watch is not non-recursive"
        );
    }

    #[test]
    fn the_debounce_window_is_500_ms() {
        assert_eq!(DEBOUNCE, Duration::from_millis(500));
    }

    #[test]
    fn only_access_events_are_dropped() {
        assert!(
            !is_change(EventKind::Access(AccessKind::Any)),
            "an access event must be dropped"
        );
        assert!(
            is_change(EventKind::Create(CreateKind::Any)),
            "a create event must rescan"
        );
        assert!(
            is_change(EventKind::Modify(ModifyKind::Any)),
            "a modify event must rescan"
        );
        assert!(
            is_change(EventKind::Remove(RemoveKind::Any)),
            "a remove event must rescan"
        );
        assert!(is_change(EventKind::Other), "an other event must rescan");
        assert!(is_change(EventKind::Any), "an any event must rescan");
    }
}
