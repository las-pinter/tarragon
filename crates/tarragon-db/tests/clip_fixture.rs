//! Check the committed `.clip` fixture with real SQLite.
//!
//! `tarragon-render` reads the embedded database without linking SQLite, and
//! its fixture builder writes the pages by hand. This test is the independent
//! oracle: SQLite opens the embedded database, runs `PRAGMA integrity_check`,
//! and reads the preview blob.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// The SQLite header magic.
const SQLITE_HEADER: &[u8] = b"SQLite format 3\0";
/// The PNG signature that starts the preview blob.
const PNG_SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";
/// The byte count of the committed preview PNG.
const PREVIEW_PNG_BYTES: usize = 5161;

/// A temporary database file that removes itself on drop.
struct TempDatabase {
    path: PathBuf,
}

impl TempDatabase {
    /// Write the database part of a `.clip` file to a unique temp file.
    fn new(clip: &[u8]) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "tarragon-clip-check-{}-{id}.sqlite",
            std::process::id()
        ));
        let offset = clip
            .windows(SQLITE_HEADER.len())
            .position(|window| window == SQLITE_HEADER)
            .expect("the fixture holds the SQLite header");
        std::fs::write(&path, &clip[offset..]).expect("the database part is written");
        Self { path }
    }

    /// Return the file path.
    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDatabase {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

#[test]
fn real_sqlite_reads_the_committed_fixture() {
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/clip/preview.clip");
    let clip = std::fs::read(&fixture).expect("the fixture reads");
    let database = TempDatabase::new(&clip);

    let connection = rusqlite::Connection::open(database.path()).expect("SQLite opens the fixture");
    let integrity: String = connection
        .query_row("PRAGMA integrity_check", [], |row| row.get(0))
        .expect("the integrity check runs");
    assert_eq!(integrity, "ok", "SQLite reports a damaged database");
    let blob: Vec<u8> = connection
        .query_row("SELECT ImageData FROM CanvasPreview LIMIT 1", [], |row| {
            row.get(0)
        })
        .expect("the preview blob reads");
    connection.close().expect("the connection closes");

    assert_eq!(blob.len(), PREVIEW_PNG_BYTES, "the preview size drifted");
    assert!(
        blob.starts_with(PNG_SIGNATURE),
        "the preview blob is not a PNG"
    );
}
