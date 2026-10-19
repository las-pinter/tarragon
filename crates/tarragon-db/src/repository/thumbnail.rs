//! Thumbnail row reads and writes.

use rusqlite::{OptionalExtension, params};
use tarragon_core::entity::FileInfo;

use crate::like::{folder_deep_like, folder_like, normalize_path};
use crate::{Database, DbError};

use super::{THUMBNAIL_COLUMNS, Thumbnail, ThumbnailWrite, row_to_thumbnail};

/// The upsert for one scan row. A conflict keeps the stored `mtime` and
/// `size` so a later render can detect a source change.
const SCAN_STUB_SQL: &str = "\
INSERT INTO thumbnails (path, mtime, size, width, height, cache_uuid, thumbnail_cache_path, preview_cache_path, full_cache_path, last_accessed)
VALUES (?1, ?2, ?3, ?4, ?5, '', NULL, NULL, NULL, 0)
ON CONFLICT(path) DO UPDATE SET mtime = thumbnails.mtime, size = thumbnails.size";

impl Database {
    /// Insert or update a full thumbnail row after a render completes.
    ///
    /// # Errors
    ///
    /// Returns [`DbError::Sqlite`] when a statement fails. Returns
    /// [`DbError::LockPoisoned`] after a panic poisoned the lock.
    pub fn upsert_thumbnail(&self, record: &ThumbnailWrite) -> Result<(), DbError> {
        let conn = self.lock()?;
        conn.execute(
            "INSERT INTO thumbnails (path, mtime, size, width, height, cache_uuid, \
             thumbnail_cache_path, preview_cache_path, full_cache_path, last_accessed) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10) \
             ON CONFLICT(path) DO UPDATE SET \
             mtime = excluded.mtime, size = excluded.size, width = excluded.width, \
             height = excluded.height, cache_uuid = excluded.cache_uuid, \
             thumbnail_cache_path = excluded.thumbnail_cache_path, \
             preview_cache_path = excluded.preview_cache_path, \
             full_cache_path = excluded.full_cache_path, \
             last_accessed = excluded.last_accessed",
            params![
                normalize_path(&record.path),
                record.mtime,
                record.size,
                record.width,
                record.height,
                record.cache_uuid,
                record.thumbnail_cache_path,
                record.preview_cache_path,
                record.full_cache_path,
                record.last_accessed,
            ],
        )?;
        Ok(())
    }

    /// Read one thumbnail row by path.
    ///
    /// # Errors
    ///
    /// Returns [`DbError::Sqlite`] when the read fails. Returns
    /// [`DbError::LockPoisoned`] after a panic poisoned the lock.
    pub fn thumbnail(&self, path: &str) -> Result<Option<Thumbnail>, DbError> {
        let conn = self.lock()?;
        let sql = format!("SELECT {THUMBNAIL_COLUMNS} FROM thumbnails WHERE path = ?1");
        let record = conn
            .query_row(&sql, [normalize_path(path)], row_to_thumbnail)
            .optional()?;
        Ok(record)
    }

    /// List every thumbnail row in least-recently-accessed-first order.
    ///
    /// The order drives the LRU eviction. The `path` column breaks a tie
    /// between equal `last_accessed` values, so the order is stable. The
    /// caller builds the eviction list from the stored cache paths.
    ///
    /// # Errors
    ///
    /// Returns [`DbError::Sqlite`] when the read fails. Returns
    /// [`DbError::LockPoisoned`] after a panic poisoned the lock.
    pub fn thumbnails_by_last_accessed(&self) -> Result<Vec<Thumbnail>, DbError> {
        let conn = self.lock()?;
        let sql = format!(
            "SELECT {THUMBNAIL_COLUMNS} FROM thumbnails ORDER BY last_accessed ASC, path ASC"
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map([], row_to_thumbnail)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    /// List every thumbnail row under `folder`, direct children only.
    ///
    /// # Errors
    ///
    /// Returns [`DbError::Sqlite`] when the read fails. Returns
    /// [`DbError::LockPoisoned`] after a panic poisoned the lock.
    pub fn thumbnails_for_folder(&self, folder: &str) -> Result<Vec<Thumbnail>, DbError> {
        let conn = self.lock()?;
        let sql = format!(
            "SELECT {THUMBNAIL_COLUMNS} FROM thumbnails \
             WHERE path LIKE ?1 ESCAPE '\\' AND path NOT LIKE ?2 ESCAPE '\\'"
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(
            [folder_like(folder), folder_deep_like(folder)],
            row_to_thumbnail,
        )?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    /// Count every thumbnail row.
    ///
    /// # Errors
    ///
    /// Returns [`DbError::Sqlite`] when the read fails. Returns
    /// [`DbError::LockPoisoned`] after a panic poisoned the lock.
    pub fn count_thumbnails(&self) -> Result<i64, DbError> {
        let conn = self.lock()?;
        let count = conn.query_row("SELECT COUNT(*) FROM thumbnails", [], |row| row.get(0))?;
        Ok(count)
    }

    /// List the distinct parent folders in the thumbnail table, sorted.
    ///
    /// # Errors
    ///
    /// Returns [`DbError::Sqlite`] when the read fails. Returns
    /// [`DbError::LockPoisoned`] after a panic poisoned the lock.
    pub fn distinct_folders(&self) -> Result<Vec<String>, DbError> {
        let conn = self.lock()?;
        let mut stmt = conn.prepare("SELECT path FROM thumbnails WHERE path != ''")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        let mut folders = std::collections::BTreeSet::new();
        for row in rows {
            let path = row?;
            if let Some(index) = path.rfind('/')
                && index > 0
            {
                folders.insert(path[..index].to_owned());
            }
        }
        Ok(folders.into_iter().collect())
    }

    /// Set `last_accessed` on many paths in one transaction.
    ///
    /// This is the batched LRU write. The method returns the number
    /// of rows it changed.
    ///
    /// # Errors
    ///
    /// Returns [`DbError::Sqlite`] when a statement fails. Returns
    /// [`DbError::LockPoisoned`] after a panic poisoned the lock.
    pub fn touch_last_accessed(&self, paths: &[String], at: i64) -> Result<usize, DbError> {
        if paths.is_empty() {
            return Ok(0);
        }
        let mut conn = self.lock()?;
        let tx = conn.transaction()?;
        let mut changed = 0;
        {
            let mut stmt =
                tx.prepare("UPDATE thumbnails SET last_accessed = ?1 WHERE path = ?2")?;
            for path in paths {
                changed += stmt.execute(params![at, normalize_path(path)])?;
            }
        }
        tx.commit()?;
        Ok(changed)
    }

    /// Delete one thumbnail row by path.
    ///
    /// # Errors
    ///
    /// Returns [`DbError::Sqlite`] when the statement fails. Returns
    /// [`DbError::LockPoisoned`] after a panic poisoned the lock.
    pub fn delete_thumbnail(&self, path: &str) -> Result<usize, DbError> {
        let conn = self.lock()?;
        let changed = conn.execute(
            "DELETE FROM thumbnails WHERE path = ?1",
            [normalize_path(path)],
        )?;
        Ok(changed)
    }

    /// Delete every thumbnail row under `folder`.
    ///
    /// # Errors
    ///
    /// Returns [`DbError::Sqlite`] when the statement fails. Returns
    /// [`DbError::LockPoisoned`] after a panic poisoned the lock.
    pub fn delete_thumbnails_by_folder(&self, folder: &str) -> Result<usize, DbError> {
        let conn = self.lock()?;
        let changed = conn.execute(
            "DELETE FROM thumbnails WHERE path LIKE ?1 ESCAPE '\\'",
            [folder_like(folder)],
        )?;
        Ok(changed)
    }

    /// Delete every thumbnail row. The cache purge calls this.
    ///
    /// # Errors
    ///
    /// Returns [`DbError::Sqlite`] when the statement fails. Returns
    /// [`DbError::LockPoisoned`] after a panic poisoned the lock.
    pub fn clear_thumbnails(&self) -> Result<usize, DbError> {
        let conn = self.lock()?;
        let changed = conn.execute("DELETE FROM thumbnails", [])?;
        Ok(changed)
    }

    /// Write one folder group of scan results in a single transaction.
    ///
    /// Each file becomes a row with the scanned width and height and no
    /// cache paths. A conflict keeps the stored `mtime` and `size`. The
    /// method returns the number of files in the group.
    ///
    /// # Errors
    ///
    /// Returns [`DbError::Sqlite`] when a statement fails. Returns
    /// [`DbError::LockPoisoned`] after a panic poisoned the lock.
    pub fn write_scan_group(&self, files: &[FileInfo]) -> Result<usize, DbError> {
        if files.is_empty() {
            return Ok(0);
        }
        let mut conn = self.lock()?;
        let tx = conn.transaction()?;
        {
            let mut stmt = tx.prepare(SCAN_STUB_SQL)?;
            for file in files {
                let path = normalize_path(&file.path.to_string_lossy());
                let size = i64::try_from(file.size).unwrap_or(i64::MAX);
                stmt.execute(params![path, file.mtime, size, file.width, file.height])?;
            }
        }
        tx.commit()?;
        Ok(files.len())
    }
}
