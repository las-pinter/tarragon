//! Editor association reads and writes.

use rusqlite::{OptionalExtension, params};

use crate::{Database, DbError};

impl Database {
    /// Read the editor command template for an extension. Missing gives `None`.
    ///
    /// # Errors
    ///
    /// Returns [`DbError::Sqlite`] when the read fails. Returns
    /// [`DbError::LockPoisoned`] after a panic poisoned the lock.
    pub fn editor_command(&self, extension: &str) -> Result<Option<String>, DbError> {
        let conn = self.lock()?;
        let command = conn
            .query_row(
                "SELECT command_template FROM editor_associations WHERE extension = ?1",
                [extension],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        Ok(command)
    }

    /// Insert or replace an editor association.
    ///
    /// # Errors
    ///
    /// Returns [`DbError::Sqlite`] when the statement fails. Returns
    /// [`DbError::LockPoisoned`] after a panic poisoned the lock.
    pub fn upsert_editor_association(
        &self,
        extension: &str,
        command_template: &str,
    ) -> Result<(), DbError> {
        let conn = self.lock()?;
        conn.execute(
            "INSERT OR REPLACE INTO editor_associations (extension, command_template) \
             VALUES (?1, ?2)",
            params![extension, command_template],
        )?;
        Ok(())
    }

    /// Remove an editor association by extension.
    ///
    /// # Errors
    ///
    /// Returns [`DbError::Sqlite`] when the statement fails. Returns
    /// [`DbError::LockPoisoned`] after a panic poisoned the lock.
    pub fn remove_editor_association(&self, extension: &str) -> Result<usize, DbError> {
        let conn = self.lock()?;
        let changed = conn.execute(
            "DELETE FROM editor_associations WHERE extension = ?1",
            [extension],
        )?;
        Ok(changed)
    }
}
