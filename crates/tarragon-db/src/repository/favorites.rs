//! Favorite row reads and writes.

use std::collections::HashSet;

use rusqlite::{TransactionBehavior, params};

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
    /// The listed paths come first, in the given order. A repeated path keeps
    /// its first place, and a path that is not a favorite is skipped. The
    /// favorites not listed follow in their old order, so no two rows tie.
    /// The method returns the number of listed favorites it placed.
    ///
    /// # Errors
    ///
    /// Returns [`DbError::Sqlite`] when a statement fails. Returns
    /// [`DbError::LockPoisoned`] after a panic poisoned the lock.
    pub fn reorder_favorites(&self, paths: &[String]) -> Result<usize, DbError> {
        let mut conn = self.lock()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let existing: Vec<String> = {
            let mut stmt = tx.prepare("SELECT path FROM favorites ORDER BY sort_order, path")?;
            let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
            rows.collect::<rusqlite::Result<Vec<_>>>()?
        };
        let known: HashSet<&str> = existing.iter().map(String::as_str).collect();
        let mut seen: HashSet<String> = HashSet::new();
        let mut ordered: Vec<String> = Vec::new();
        for path in paths {
            let normalized = normalize_path(path);
            if known.contains(normalized.as_str()) && seen.insert(normalized.clone()) {
                ordered.push(normalized);
            }
        }
        let placed = ordered.len();
        ordered.extend(
            existing
                .iter()
                .filter(|path| !seen.contains(*path))
                .cloned(),
        );
        {
            let mut stmt = tx.prepare("UPDATE favorites SET sort_order = ?1 WHERE path = ?2")?;
            for (index, path) in ordered.iter().enumerate() {
                let order = i64::try_from(index).unwrap_or(i64::MAX);
                stmt.execute(params![order, path])?;
            }
        }
        tx.commit()?;
        Ok(placed)
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
