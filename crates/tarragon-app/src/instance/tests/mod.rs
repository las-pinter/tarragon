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
        let dir = std::env::temp_dir().join(format!(
            "tarragon-instance-test-{}-{id}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).expect("create the temp dir");
        Self(dir)
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
