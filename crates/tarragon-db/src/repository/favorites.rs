//! Favorite row reads and writes.

use rusqlite::params;

use crate::like::normalize_path;
use crate::{Database, DbError};

use super::{Favorite, row_to_favorite};

impl Database {
    /// Add a favorite if the path is not present.
    ///
    /// # Errors
    ///
    /// Returns [`DbError::Sqlite`] when the statement fails. Returns
    /// [`DbError::LockPoisoned`] after a panic poisoned the lock.
    pub fn add_favorite(
        &self,
        path: &str,
        label: Option<&str>,
        sort_order: i64,
    ) -> Result<usize, DbError> {
        let conn = self.lock()?;
        let changed = conn.execute(
            "INSERT OR IGNORE INTO favorites (path, label, sort_order) VALUES (?1, ?2, ?3)",
            params![normalize_path(path), label, sort_order],
        )?;
        Ok(changed)
    }

    /// Remove a favorite by path.
    ///
    /// # Errors
    ///
    /// Returns [`DbError::Sqlite`] when the statement fails. Returns
    /// [`DbError::LockPoisoned`] after a panic poisoned the lock.
    pub fn remove_favorite(&self, path: &str) -> Result<usize, DbError> {
        let conn = self.lock()?;
        let changed = conn.execute(
            "DELETE FROM favorites WHERE path = ?1",
            [normalize_path(path)],
        )?;
        Ok(changed)
    }

    /// Set the sort order of one favorite.
    ///
    /// # Errors
    ///
    /// Returns [`DbError::Sqlite`] when the statement fails. Returns
    /// [`DbError::LockPoisoned`] after a panic poisoned the lock.
    pub fn update_favorite_sort_order(
        &self,
        path: &str,
        sort_order: i64,
    ) -> Result<usize, DbError> {
        let conn = self.lock()?;
        let changed = conn.execute(
            "UPDATE favorites SET sort_order = ?1 WHERE path = ?2",
            params![sort_order, normalize_path(path)],
        )?;
        Ok(changed)
    }

    /// Reindex favorite sort orders to `0..n-1` in one transaction.
    ///
    /// # Errors
    ///
    /// Returns [`DbError::Sqlite`] when a statement fails. Returns
    /// [`DbError::LockPoisoned`] after a panic poisoned the lock.
    pub fn reorder_favorites(&self, paths: &[String]) -> Result<usize, DbError> {
        let mut conn = self.lock()?;
        let tx = conn.transaction()?;
        let mut changed = 0;
        {
            let mut stmt = tx.prepare("UPDATE favorites SET sort_order = ?1 WHERE path = ?2")?;
            for (index, path) in paths.iter().enumerate() {
                let order = i64::try_from(index).unwrap_or(i64::MAX);
                changed += stmt.execute(params![order, normalize_path(path)])?;
            }
        }
        tx.commit()?;
        Ok(changed)
    }

    /// List every favorite ordered by sort order, then path.
    ///
    /// # Errors
    ///
    /// Returns [`DbError::Sqlite`] when the read fails. Returns
    /// [`DbError::LockPoisoned`] after a panic poisoned the lock.
    pub fn list_favorites(&self) -> Result<Vec<Favorite>, DbError> {
        let conn = self.lock()?;
        let mut stmt = conn.prepare(
            "SELECT id, path, label, sort_order FROM favorites ORDER BY sort_order, path",
        )?;
        let rows = stmt.query_map([], row_to_favorite)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }
}
