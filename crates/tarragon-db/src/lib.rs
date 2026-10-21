//! SQLite storage layer for tarragon: schema and repositories.
//!
//! The crate uses `rusqlite` with the `bundled` feature. It performs I/O. It
//! depends on `tarragon-core` only and does not depend on the GUI.
//!
//! The app keeps one connection on one database thread. [`Database`] guards
//! that connection with a mutex so the type is `Send + Sync`.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use rusqlite::Connection;

mod like;
pub mod query;
pub mod repository;
pub mod schema;
pub mod settings;

#[cfg(test)]
mod tests;

/// A database error.
#[derive(Debug, thiserror::Error)]
pub enum DbError {
    /// SQLite refused a statement.
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    /// The database file cannot open.
    #[error("cannot open database {path}: {source}")]
    Open {
        /// The database file path.
        path: PathBuf,
        /// The cause.
        #[source]
        source: rusqlite::Error,
    },
    /// WAL journal mode did not turn on.
    #[error("could not enable WAL journal mode, got {0}")]
    Wal(String),
    /// The shared connection lock is poisoned after a panic.
    #[error("database connection lock is poisoned")]
    LockPoisoned,
    /// A shared domain value is invalid.
    #[error(transparent)]
    Core(#[from] tarragon_core::error::CoreError),
    /// The stored window state text is not valid window state.
    #[error("invalid window state: {0}")]
    WindowState(#[from] serde_json::Error),
}

/// One SQLite connection with the fixed connection settings.
///
/// This type is the only connect site. Opening sets WAL, `foreign_keys=ON`,
/// and a five second busy timeout.
pub struct Database {
    conn: Mutex<Connection>,
}

impl Database {
    /// Open the database file at `path` and configure the connection.
    ///
    /// # Errors
    ///
    /// Returns [`DbError::Open`] when the file cannot open. Returns
    /// [`DbError::Wal`] when WAL does not turn on. Returns
    /// [`DbError::Sqlite`] when a pragma fails.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, DbError> {
        let path = path.as_ref();
        let conn = Connection::open(path).map_err(|source| DbError::Open {
            path: path.to_path_buf(),
            source,
        })?;
        Self::configure(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// Set WAL, turn foreign keys on, and set the busy timeout.
    fn configure(conn: &Connection) -> Result<(), DbError> {
        let mode: String = conn.query_row("PRAGMA journal_mode=WAL", [], |row| row.get(0))?;
        if !mode.eq_ignore_ascii_case("wal") {
            return Err(DbError::Wal(mode));
        }
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.busy_timeout(Duration::from_secs(5))?;
        Ok(())
    }

    /// Create the schema and stamp the generation marker.
    ///
    /// The call is idempotent. It returns the stored generation.
    ///
    /// # Errors
    ///
    /// Returns [`DbError::Sqlite`] when a statement fails.
    pub fn initialize(&self) -> Result<i64, DbError> {
        let mut conn = self.lock()?;
        schema::initialize(&mut conn)
    }

    /// Lock the shared connection.
    ///
    /// # Errors
    ///
    /// Returns [`DbError::LockPoisoned`] after a panic poisoned the lock.
    pub(crate) fn lock(&self) -> Result<MutexGuard<'_, Connection>, DbError> {
        self.conn.lock().map_err(|_| DbError::LockPoisoned)
    }
}

impl std::fmt::Debug for Database {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("Database").finish_non_exhaustive()
    }
}
