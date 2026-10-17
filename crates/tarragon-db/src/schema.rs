//! Database schema: the eight tables, the generation marker, and creation.
//!
//! The schema is one generation, not a migration chain. Creation is
//! idempotent, so the marker only records which shape is present. The app
//! uses a fresh database and never reads or migrates data from an earlier
//! schema.

use rusqlite::{Connection, OptionalExtension};

use crate::DbError;

/// The current schema generation.
///
/// The schema generation is `2`. The table shape adds
/// `thumbnails.last_accessed`.
pub const SCHEMA_VERSION: i64 = 2;

/// The `CREATE TABLE IF NOT EXISTS` statements for the eight tables.
///
/// `file_tags.tag_id` carries the only foreign key. It cascades a tag delete
/// (`ON DELETE CASCADE`). `thumbnails.last_accessed` holds epoch seconds for
/// the LRU order.
pub const INITIAL_SCHEMA: &str = "\
CREATE TABLE IF NOT EXISTS schema_version (version INTEGER NOT NULL);
CREATE TABLE IF NOT EXISTS thumbnails (
    path TEXT PRIMARY KEY,
    mtime INTEGER NOT NULL,
    size INTEGER NOT NULL,
    thumb_hash TEXT,
    width INTEGER NOT NULL,
    height INTEGER NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    cache_uuid TEXT,
    thumbnail_cache_path TEXT,
    preview_cache_path TEXT,
    full_cache_path TEXT,
    last_accessed INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE IF NOT EXISTS tags (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT UNIQUE NOT NULL,
    source TEXT NOT NULL DEFAULT 'user'
);
CREATE TABLE IF NOT EXISTS file_tags (
    path TEXT NOT NULL,
    tag_id INTEGER NOT NULL,
    source TEXT NOT NULL DEFAULT 'user',
    PRIMARY KEY (path, tag_id),
    FOREIGN KEY (tag_id) REFERENCES tags(id) ON DELETE CASCADE
);
CREATE TABLE IF NOT EXISTS favorites (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    path TEXT UNIQUE NOT NULL,
    label TEXT,
    sort_order INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE IF NOT EXISTS settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS editor_associations (
    extension TEXT PRIMARY KEY,
    command_template TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS folder_cache_uuids (
    folder_path TEXT PRIMARY KEY,
    cache_uuid TEXT NOT NULL
);
";

/// Create the eight tables and stamp the generation marker.
///
/// The whole call is one transaction, so it is atomic and idempotent. It
/// returns the stored generation.
///
/// # Errors
///
/// Returns [`DbError::Sqlite`] when a statement fails.
pub fn initialize(conn: &mut Connection) -> Result<i64, DbError> {
    let tx = conn.transaction()?;
    tx.execute_batch(INITIAL_SCHEMA)?;
    tx.execute("DELETE FROM schema_version", [])?;
    tx.execute(
        "INSERT INTO schema_version (version) VALUES (?1)",
        [SCHEMA_VERSION],
    )?;
    tx.commit()?;
    Ok(SCHEMA_VERSION)
}

/// Read the stored generation marker.
///
/// Returns `0` when no marker row exists.
///
/// # Errors
///
/// Returns [`DbError::Sqlite`] when the read fails.
pub fn version(conn: &Connection) -> Result<i64, DbError> {
    let stored = conn
        .query_row("SELECT version FROM schema_version LIMIT 1", [], |row| {
            row.get::<_, i64>(0)
        })
        .optional()?;
    Ok(stored.unwrap_or(0))
}

impl crate::Database {
    /// Read the stored generation marker.
    ///
    /// Returns `0` when no marker row exists.
    ///
    /// # Errors
    ///
    /// Returns [`DbError::Sqlite`] when the read fails. Returns
    /// [`DbError::LockPoisoned`] after a panic poisoned the lock.
    pub fn schema_version(&self) -> Result<i64, DbError> {
        let conn = self.lock()?;
        version(&conn)
    }
}
