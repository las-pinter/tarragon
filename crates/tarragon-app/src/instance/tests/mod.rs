//! Tests for the guard, split by topic.

mod hostile;
mod lifecycle;

use std::sync::atomic::{AtomicU64, Ordering};

use super::paths::Location;
use super::*;

/// A temporary folder that removes itself on drop.
struct TempDir(pub(super) PathBuf);

impl TempDir {
    pub(super) fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        // A short path: the unix tests bind a socket in this folder, and the
        // macOS temp folder is long, so unix uses `/tmp`.
        let base = if cfg!(unix) {
            PathBuf::from("/tmp")
        } else {
            std::env::temp_dir()
        };
        let dir = base.join(format!("ti-{}-{id}", tarragon_core::testing::run_tag()));
        std::fs::create_dir_all(&dir).expect("create the temp dir");
        let made = Self(dir);
        // Only unix binds a socket in this folder.
        if cfg!(unix) {
            assert!(
                made.layout().socket_path_fits(),
                "the socket path is too long: {}",
                made.layout().sock.display()
            );
        }
        made
    }

    /// The instance files for this folder.
    pub(super) fn layout(&self) -> Layout {
        Layout::new(&Location::Private(self.0.clone()), "0123456789abcdef")
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Unwrap a Primary or fail the test.
pub(super) fn primary(outcome: Outcome) -> Guard {
    match outcome {
        Outcome::Primary(guard) => guard,
        Outcome::Forwarded => panic!("expected Primary, got Forwarded"),
        Outcome::Unguarded(reason) => panic!("expected Primary, got Unguarded: {reason}"),
    }
}
