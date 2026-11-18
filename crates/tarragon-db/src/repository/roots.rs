//! Library root reads and writes.
//!
//! A root is stored like every other path: backslashes become forward
//! slashes. Trailing slashes are trimmed, so `/a/` and `/a` are one root,
//! as the folder filter also ignores them. A filesystem root (`/`) and a bare
//! drive (`C:/`) keep their slash, because the trimmed text would name
//! something else. The text is otherwise kept as given, so the compare is
//! bytewise and case-sensitive, as in the folder filter.

use rusqlite::params;

use crate::like::normalize_path;
use crate::{Database, DbError};

/// Return the stored form of a root path.
fn root_key(path: &str) -> String {
    let normalized = normalize_path(path);
    let trimmed = normalized.trim_end_matches('/');
    let bytes = trimmed.as_bytes();
    let bare_drive = bytes.len() == 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':';
    if trimmed.is_empty() || bare_drive {
        return normalized;
    }
    trimmed.to_owned()
}

impl Database {
    /// List every library root ordered by sort order, then path.
    ///
    /// # Errors
    ///
    /// Returns [`DbError::Sqlite`] when the read fails. Returns
    /// [`DbError::LockPoisoned`] after a panic poisoned the lock.
    pub fn list_library_roots(&self) -> Result<Vec<String>, DbError> {
        let conn = self.lock()?;
        let mut stmt = conn.prepare("SELECT path FROM library_roots ORDER BY sort_order, path")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    /// Add a library root after the last one if the path is not present.
    ///
    /// Returns `true` when a row was added and `false` when the path was
    /// already a root.
    ///
    /// # Errors
    ///
    /// Returns [`DbError::Sqlite`] when the statement fails. Returns
    /// [`DbError::LockPoisoned`] after a panic poisoned the lock.
    pub fn add_library_root(&self, path: &str) -> Result<bool, DbError> {
        let conn = self.lock()?;
        let changed = conn.execute(
            "INSERT OR IGNORE INTO library_roots (path, sort_order) \
             VALUES (?1, (SELECT COALESCE(MAX(sort_order), -1) + 1 FROM library_roots))",
            params![root_key(path)],
        )?;
        Ok(changed > 0)
    }

    /// Remove a library root by path.
    ///
    /// Returns the number of rows removed.
    ///
    /// # Errors
    ///
    /// Returns [`DbError::Sqlite`] when the statement fails. Returns
    /// [`DbError::LockPoisoned`] after a panic poisoned the lock.
    pub fn remove_library_root(&self, path: &str) -> Result<usize, DbError> {
        let conn = self.lock()?;
        let changed = conn.execute(
            "DELETE FROM library_roots WHERE path = ?1",
            [root_key(path)],
        )?;
        Ok(changed)
    }
}
