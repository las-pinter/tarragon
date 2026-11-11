//! Move a file's rows to a new path.

use rusqlite::params;

use crate::like::normalize_path;
use crate::{Database, DbError};

impl Database {
    /// Rename a file in every table that stores its path.
    ///
    /// One transaction deletes any rows at `new`, then updates `thumbnails`,
    /// `file_tags`, and `favorites` from `old` to `new`. The thumbnail row
    /// loses its cache links and `thumb_hash`, because the cache file names
    /// follow the file name. Its `mtime`, `size`, `width`, and `height` stay,
    /// and so do the tags and the favorite sort order. A table with no row for
    /// `old` is skipped. A rename to the same path changes nothing.
    ///
    /// The method replaces every row at `new` and does not check the disk, so
    /// the caller must check that no file exists at `new` first. A replaced
    /// `thumbnails` row can leave cache files behind. The caller removes them.
    ///
    /// # Errors
    ///
    /// Returns [`DbError::Sqlite`] when a statement fails. Returns
    /// [`DbError::LockPoisoned`] after a panic poisoned the lock.
    pub fn rename_file(&self, old: &str, new: &str) -> Result<(), DbError> {
        let old = normalize_path(old);
        let new = normalize_path(new);
        if old == new {
            return Ok(());
        }
        let mut conn = self.lock()?;
        let tx = conn.transaction()?;
        tx.execute("DELETE FROM thumbnails WHERE path = ?1", [&new])?;
        tx.execute("DELETE FROM file_tags WHERE path = ?1", [&new])?;
        tx.execute("DELETE FROM favorites WHERE path = ?1", [&new])?;
        tx.execute(
            "UPDATE thumbnails SET path = ?2, thumb_hash = NULL, \
             thumbnail_cache_path = NULL, preview_cache_path = NULL, \
             full_cache_path = NULL WHERE path = ?1",
            params![old, new],
        )?;
        tx.execute(
            "UPDATE file_tags SET path = ?2 WHERE path = ?1",
            params![old, new],
        )?;
        tx.execute(
            "UPDATE favorites SET path = ?2 WHERE path = ?1",
            params![old, new],
        )?;
        tx.commit()?;
        Ok(())
    }
}
