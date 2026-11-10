//! One transaction for a whole writer batch.

use crate::{Database, DbError};

use super::ThumbnailWrite;
use super::tags::replace_auto_colors;
use super::thumbnail::{touch_path, write_thumbnail};

/// The writes one writer pass stores together.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WriteBatch {
    /// Rendered rows to insert or update.
    pub rows: Vec<ThumbnailWrite>,
    /// Paths whose `last_accessed` is set to `touched_at`.
    pub touched: Vec<String>,
    /// The `last_accessed` value for every touched path, in epoch seconds.
    pub touched_at: i64,
    /// Paths with the colour tag names that replace their `auto_color` tags.
    pub colors: Vec<(String, Vec<String>)>,
}

impl WriteBatch {
    /// Return true when the batch holds no item.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty() && self.touched.is_empty() && self.colors.is_empty()
    }
}

/// The outcome of [`Database::apply_writes`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WriteReport {
    /// True when at least one item changed the database.
    pub stored: bool,
    /// The number of items that were rolled back.
    pub failed: usize,
}

impl Database {
    /// Store a whole batch in one transaction.
    ///
    /// The method runs the rows, then the touches, then the colours, and
    /// commits once. An empty batch opens no transaction. Each item runs in
    /// its own savepoint, so a failed item rolls back alone, is counted in
    /// [`WriteReport::failed`], and does not stop the others.
    ///
    /// # Errors
    ///
    /// Returns [`DbError::Sqlite`] when the transaction cannot open or
    /// commit, or when an item error ended the whole transaction. In that
    /// case the call stops and nothing of the batch is stored. Returns
    /// [`DbError::LockPoisoned`] after a panic poisoned the lock.
    pub fn apply_writes(&self, batch: &WriteBatch) -> Result<WriteReport, DbError> {
        let mut report = WriteReport::default();
        if batch.is_empty() {
            return Ok(report);
        }
        let mut conn = self.lock()?;
        let mut tx = conn.transaction()?;
        for row in &batch.rows {
            let result = in_savepoint(&mut tx, |conn| write_thumbnail(conn, row))?;
            record(&mut report, &row.path, result.map(|()| true));
        }
        for path in &batch.touched {
            let result = in_savepoint(&mut tx, |conn| touch_path(conn, path, batch.touched_at))?;
            record(&mut report, path, result.map(|changed| changed > 0));
        }
        for (path, names) in &batch.colors {
            let result = in_savepoint(&mut tx, |conn| replace_auto_colors(conn, path, names))?;
            record(&mut report, path, result.map(|_| true));
        }
        tx.commit()?;
        Ok(report)
    }
}

/// Run `item` in a savepoint. An item error rolls the savepoint back and
/// comes back as the inner result, so the caller counts it.
///
/// The outer result is an error when the whole transaction is gone. A
/// `RAISE(ROLLBACK)` trigger or a disk error can end it, and the savepoint
/// drop hides that. The next item would then run in autocommit, outside the
/// batch, so the call stops with the item error instead.
fn in_savepoint<T>(
    tx: &mut rusqlite::Transaction<'_>,
    item: impl FnOnce(&rusqlite::Connection) -> Result<T, DbError>,
) -> Result<Result<T, DbError>, DbError> {
    let savepoint = tx.savepoint()?;
    match item(&savepoint) {
        Ok(value) => {
            savepoint.commit()?;
            Ok(Ok(value))
        }
        Err(error) => {
            drop(savepoint);
            if tx.is_autocommit() {
                return Err(error);
            }
            Ok(Err(error))
        }
    }
}

/// Add one item result to the report and log a failure.
fn record(report: &mut WriteReport, path: &str, result: Result<bool, DbError>) {
    match result {
        Ok(changed) => report.stored |= changed,
        Err(error) => {
            report.failed += 1;
            tracing::warn!("cannot store the batch item for {path}: {error}");
        }
    }
}
