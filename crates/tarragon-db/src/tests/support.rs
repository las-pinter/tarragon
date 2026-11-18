use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use tarragon_core::entity::FileInfo;

use crate::Database;

/// A temporary database file that removes itself on drop.
pub(super) struct TempDb {
    dir: PathBuf,
    pub(super) db: Database,
}

impl TempDb {
    /// Create and initialize a fresh database in a unique temp directory.
    pub(super) fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("tarragon-db-test-{}-{id}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("create the temp dir");
        let db = Database::open(dir.join("tarragon.db")).expect("open the temp database");
        db.initialize().expect("create the schema");
        Self { dir, db }
    }

    /// The database file path, for a second connection.
    pub(super) fn path(&self) -> PathBuf {
        self.dir.join("tarragon.db")
    }
}

impl Drop for TempDb {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// Build a scan file under a folder.
pub(super) fn file(folder: &str, name: &str, mtime: i64, size: u64) -> FileInfo {
    sized_file(folder, name, mtime, size, 0, 0)
}

/// Build a scan file under a folder with known pixel dimensions.
pub(super) fn sized_file(
    folder: &str,
    name: &str,
    mtime: i64,
    size: u64,
    width: u32,
    height: u32,
) -> FileInfo {
    FileInfo {
        path: PathBuf::from(format!("{folder}/{name}")),
        size,
        mtime,
        width,
        height,
    }
}
