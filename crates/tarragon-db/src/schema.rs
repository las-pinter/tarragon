//! Database schema: the eight tables, the generation marker, and creation.
//!
//! The schema is one generation, not a migration chain. The marker gates
//! creation: a new database is created and stamped, a matching one is left
//! alone, and any other marker is refused. The app never reads or migrates
//! data from an earlier schema.

use rusqlite::{Connection, OptionalExtension, TransactionBehavior};

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
/// The marker is read in the same transaction as the create. A marker of `0`
/// means a new database, so the tables are created and the marker is stamped.
/// A marker equal to [`SCHEMA_VERSION`] changes nothing. Any other marker is
/// refused, because `CREATE TABLE IF NOT EXISTS` would keep an old table
/// shape and a newer build's data must not be stamped down. It returns the
/// stored generation.
///
/// # Errors
///
/// Returns [`DbError::Sqlite`] when a statement fails. Returns
/// [`DbError::SchemaVersion`] when the stored marker is not `0` and not
/// [`SCHEMA_VERSION`].
pub fn initialize(conn: &mut Connection) -> Result<i64, DbError> {
    // An immediate transaction takes the write lock first, so two processes
    // cannot both read `0` and both create.
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    tx.execute_batch("CREATE TABLE IF NOT EXISTS schema_version (version INTEGER NOT NULL);")?;
    let found = version(&tx)?;
    if found == SCHEMA_VERSION {
        return Ok(found);
    }
    if found != 0 {
        return Err(DbError::SchemaVersion {
            found,
            expected: SCHEMA_VERSION,
        });
    }
    tx.execute_batch(INITIAL_SCHEMA)?;
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
