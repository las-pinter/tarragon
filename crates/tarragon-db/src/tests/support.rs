use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use tarragon_core::entity::FileInfo;

use crate::Database;

/// A temporary database file that removes itself on drop.
///
/// Fields drop in order, so the connection closes before the folder goes.
/// Windows cannot delete a folder that holds an open database.
pub(super) struct TempDb {
    pub(super) db: Database,
    dir: DirGuard,
}

/// Removes the folder when dropped.
struct DirGuard(PathBuf);

impl Drop for DirGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

impl TempDb {
    /// Create and initialize a fresh database in a unique temp directory.
    pub(super) fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "tarragon-db-test-{}-{id}",
            tarragon_core::testing::run_tag()
        ));
        // Remove a leftover folder of the same name, should one exist.
        // A folder that cannot go would hand this test old rows, so fail.
        match std::fs::remove_dir_all(&dir) {
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
                panic!("cannot remove the stale folder {}: {error}", dir.display());
            }
            _ => {}
        }
        std::fs::create_dir_all(&dir).expect("create the temp dir");
        let db = Database::open(dir.join("tarragon.db")).expect("open the temp database");
        db.initialize().expect("create the schema");
        Self {
            db,
            dir: DirGuard(dir),
        }
    }

    /// The database file path, for a second connection.
    pub(super) fn path(&self) -> PathBuf {
        self.dir.0.join("tarragon.db")
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
