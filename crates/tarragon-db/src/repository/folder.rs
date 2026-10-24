//! Folder cache UUID reads and writes.

use std::path::Path;

use rusqlite::{OptionalExtension, params};

use crate::like::normalize_path;
use crate::{Database, DbError};

impl Database {
    /// Read the cache UUID of a source folder. Missing gives `None`.
    ///
    /// # Errors
    ///
    /// Returns [`DbError::Sqlite`] when the read fails. Returns
    /// [`DbError::LockPoisoned`] after a panic poisoned the lock.
    pub fn folder_uuid(&self, folder: &str) -> Result<Option<String>, DbError> {
        let conn = self.lock()?;
        let uuid = conn
            .query_row(
                "SELECT cache_uuid FROM folder_cache_uuids WHERE folder_path = ?1",
                [normalize_path(folder)],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        Ok(uuid)
    }

    /// Insert or update the cache UUID of a source folder.
    ///
    /// # Errors
    ///
    /// Returns [`DbError::Sqlite`] when the statement fails. Returns
    /// [`DbError::LockPoisoned`] after a panic poisoned the lock.
    pub fn upsert_folder_uuid(&self, folder: &str, cache_uuid: &str) -> Result<(), DbError> {
        let conn = self.lock()?;
        conn.execute(
            "INSERT INTO folder_cache_uuids (folder_path, cache_uuid) VALUES (?1, ?2) \
             ON CONFLICT(folder_path) DO UPDATE SET cache_uuid = excluded.cache_uuid",
            params![normalize_path(folder), cache_uuid],
        )?;
        Ok(())
    }

    /// Insert `candidate` for a folder and return the stored UUID.
    ///
    /// The insert and the read-back run in one transaction, so racing
    /// callers converge on one UUID.
    ///
    /// # Errors
    ///
    /// Returns [`DbError::Sqlite`] when a statement fails. Returns
    /// [`DbError::LockPoisoned`] after a panic poisoned the lock.
    pub fn get_or_create_folder_uuid(
        &self,
        folder: &str,
        candidate: &str,
    ) -> Result<String, DbError> {
        let normalized = normalize_path(folder);
        let mut conn = self.lock()?;
        let tx = conn.transaction()?;
        tx.execute(
            "INSERT INTO folder_cache_uuids (folder_path, cache_uuid) VALUES (?1, ?2) \
             ON CONFLICT(folder_path) DO NOTHING",
            params![normalized, candidate],
        )?;
        let uuid = tx.query_row(
            "SELECT cache_uuid FROM folder_cache_uuids WHERE folder_path = ?1",
            [&normalized],
            |row| row.get::<_, String>(0),
        )?;
        tx.commit()?;
        Ok(uuid)
    }

    /// Remove cache UUID rows whose source folder is gone.
    ///
    /// The method returns the number of rows it removed.
    ///
    /// # Errors
    ///
    /// Returns [`DbError::Sqlite`] when a statement fails. Returns
    /// [`DbError::LockPoisoned`] after a panic poisoned the lock.
    pub fn cleanup_stale_folder_uuids(&self) -> Result<usize, DbError> {
        let rows: Vec<(String, String)> = {
            let conn = self.lock()?;
            let mut stmt =
                conn.prepare("SELECT folder_path, cache_uuid FROM folder_cache_uuids")?;
            let rows = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
            rows.collect::<rusqlite::Result<Vec<_>>>()?
        };
        let stale: Vec<(String, String)> = rows
            .into_iter()
            .filter(|(folder, _)| !Path::new(folder).is_dir())
            .collect();
        if stale.is_empty() {
            return Ok(0);
        }
        let mut conn = self.lock()?;
        let tx = conn.transaction()?;
        let mut removed = 0;
        {
            // The lock is free during the directory checks. Matching the UUID
            // too keeps a row that another caller replaced in that gap.
            let mut stmt = tx.prepare(
                "DELETE FROM folder_cache_uuids WHERE folder_path = ?1 AND cache_uuid = ?2",
            )?;
            for (folder, uuid) in &stale {
                removed += stmt.execute([folder, uuid])?;
            }
        }
        tx.commit()?;
        Ok(removed)
    }
}
