//! Folder cache UUID reads and writes.

use std::collections::HashMap;
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

    /// Return the stored UUID of each folder, and create the missing ones.
    ///
    /// `new_id` makes the UUID of a folder that has no row. It runs once for
    /// each missing folder and not for a stored one. All reads and inserts run
    /// in one transaction, so the call commits once. The map has one entry for
    /// each distinct text in `folders`.
    ///
    /// # Errors
    ///
    /// Returns [`DbError::Sqlite`] when a statement fails. Returns
    /// [`DbError::LockPoisoned`] after a panic poisoned the lock.
    pub fn get_or_create_folder_uuids(
        &self,
        folders: &[String],
        mut new_id: impl FnMut() -> String,
    ) -> Result<HashMap<String, String>, DbError> {
        let mut conn = self.lock()?;
        let tx = conn.transaction()?;
        let mut ids = HashMap::with_capacity(folders.len());
        {
            let mut read =
                tx.prepare("SELECT cache_uuid FROM folder_cache_uuids WHERE folder_path = ?1")?;
            let mut insert = tx.prepare(
                "INSERT INTO folder_cache_uuids (folder_path, cache_uuid) VALUES (?1, ?2)",
            )?;
            // Two texts can share one normalized path, so they share one row.
            let mut by_path: HashMap<String, String> = HashMap::new();
            for folder in folders {
                if ids.contains_key(folder) {
                    continue;
                }
                let normalized = normalize_path(folder);
                let uuid = if let Some(uuid) = by_path.get(&normalized) {
                    uuid.clone()
                } else {
                    let stored = read
                        .query_row([&normalized], |row| row.get::<_, String>(0))
                        .optional()?;
                    let uuid = if let Some(uuid) = stored {
                        uuid
                    } else {
                        let uuid = new_id();
                        insert.execute(params![normalized, uuid])?;
                        uuid
                    };
                    by_path.insert(normalized, uuid.clone());
                    uuid
                };
                ids.insert(folder.clone(), uuid);
            }
        }
        tx.commit()?;
        Ok(ids)
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
