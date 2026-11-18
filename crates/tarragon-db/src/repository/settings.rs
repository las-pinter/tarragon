//! Raw string setting access.

use rusqlite::{OptionalExtension, params};

use crate::{Database, DbError};

impl Database {
    /// Read a raw string setting. A missing key gives `None`.
    ///
    /// # Errors
    ///
    /// Returns [`DbError::Sqlite`] when the read fails. Returns
    /// [`DbError::LockPoisoned`] after a panic poisoned the lock.
    pub fn setting(&self, key: &str) -> Result<Option<String>, DbError> {
        let conn = self.lock()?;
        let value = conn
            .query_row("SELECT value FROM settings WHERE key = ?1", [key], |row| {
                row.get::<_, String>(0)
            })
            .optional()?;
        Ok(value)
    }

    /// Store a raw string setting.
    ///
    /// # Errors
    ///
    /// Returns [`DbError::Sqlite`] when the statement fails. Returns
    /// [`DbError::LockPoisoned`] after a panic poisoned the lock.
    pub fn set_setting(&self, key: &str, value: &str) -> Result<(), DbError> {
        let conn = self.lock()?;
        conn.execute(
            "INSERT OR REPLACE INTO settings (key, value) VALUES (?1, ?2)",
            params![key, value],
        )?;
        Ok(())
    }

    /// Store several raw string settings in one transaction.
    ///
    /// Either every pair is stored or none is. A repeated key keeps the last
    /// value. An empty slice does nothing.
    ///
    /// # Errors
    ///
    /// Returns [`DbError::Sqlite`] when a statement fails. Returns
    /// [`DbError::LockPoisoned`] after a panic poisoned the lock.
    pub fn set_settings(&self, pairs: &[(&str, &str)]) -> Result<(), DbError> {
        if pairs.is_empty() {
            return Ok(());
        }
        let mut conn = self.lock()?;
        let tx = conn.transaction()?;
        {
            let mut stmt =
                tx.prepare("INSERT OR REPLACE INTO settings (key, value) VALUES (?1, ?2)")?;
            for (key, value) in pairs {
                stmt.execute(params![key, value])?;
            }
        }
        tx.commit()?;
        Ok(())
    }
}
