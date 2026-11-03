//! Tag and file-tag reads and writes.

use std::collections::{HashMap, HashSet};
use std::str::FromStr;

use rusqlite::{OptionalExtension, params};
use tarragon_core::entity::{Tag, TagSource};
use tarragon_core::tag_name::{TagNameError, validate_tag_name};

use crate::like::normalize_path;
use crate::{Database, DbError};

use super::{PATH_CHUNK, TagUsage, ensure_tag_conn, placeholders};

impl Database {
    /// Insert a tag if absent and return it.
    ///
    /// The stored `source` never changes on conflict. The returned tag keeps
    /// the requested `source`, which callers use for the association row.
    ///
    /// A `user` tag may not be named like a color bucket or `transparent`. The
    /// check ignores ASCII case. This method checks only that rule. The caller
    /// trims the name and checks that it is not empty or too long.
    ///
    /// # Errors
    ///
    /// Returns [`DbError::ReservedTagName`] for a reserved `user` name.
    /// Returns [`DbError::Sqlite`] when the statement fails. Returns
    /// [`DbError::LockPoisoned`] after a panic poisoned the lock.
    pub fn ensure_tag(&self, name: &str, source: TagSource) -> Result<Tag, DbError> {
        let conn = self.lock()?;
        ensure_tag_conn(&conn, name, source)
    }

    /// Associate every tag with every path in one transaction.
    ///
    /// A `user` pair promotes an existing row to user-owned. An `auto_color`
    /// pair never downgrades a user row. The method returns the number of
    /// pairs it wrote.
    ///
    /// # Errors
    ///
    /// Returns [`DbError::Sqlite`] when a statement fails. Returns
    /// [`DbError::LockPoisoned`] after a panic poisoned the lock.
    pub fn add_tags_to_files_batch(
        &self,
        paths: &[String],
        tags: &[Tag],
    ) -> Result<usize, DbError> {
        if paths.is_empty() || tags.is_empty() {
            return Ok(0);
        }
        let mut conn = self.lock()?;
        let tx = conn.transaction()?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO file_tags (path, tag_id, source) VALUES (?1, ?2, ?3) \
                 ON CONFLICT(path, tag_id) DO UPDATE SET \
                 source = CASE WHEN excluded.source = 'user' THEN 'user' ELSE file_tags.source END",
            )?;
            for path in paths {
                let normalized = normalize_path(path);
                for tag in tags {
                    stmt.execute(params![normalized, tag.id, tag.source.as_str()])?;
                }
            }
        }
        tx.commit()?;
        Ok(paths.len() * tags.len())
    }

    /// Read the tags of many paths. Every requested path gets an entry.
    ///
    /// The path list is split into chunks of [`PATH_CHUNK`], so a large
    /// library cannot exceed the SQLite variable limit. The chunk results
    /// merge into one map.
    ///
    /// # Errors
    ///
    /// Returns [`DbError::Sqlite`] when the read fails. Returns
    /// [`DbError::Core`] when a stored tag source is unknown. Returns
    /// [`DbError::LockPoisoned`] after a panic poisoned the lock.
    pub fn tags_for_files(&self, paths: &[String]) -> Result<HashMap<String, Vec<Tag>>, DbError> {
        let normalized: Vec<String> = paths.iter().map(|path| normalize_path(path)).collect();
        let mut out: HashMap<String, Vec<Tag>> = normalized
            .iter()
            .map(|path| (path.clone(), Vec::new()))
            .collect();
        if normalized.is_empty() {
            return Ok(out);
        }
        let conn = self.lock()?;
        for chunk in normalized.chunks(PATH_CHUNK) {
            let sql = format!(
                "SELECT ft.path AS path, t.id AS id, t.name AS name, t.source AS source \
                 FROM file_tags ft JOIN tags t ON t.id = ft.tag_id \
                 WHERE ft.path IN ({}) ORDER BY t.name",
                placeholders(chunk.len())
            );
            let mut stmt = conn.prepare(&sql)?;
            let params: Vec<&dyn rusqlite::ToSql> = chunk
                .iter()
                .map(|path| path as &dyn rusqlite::ToSql)
                .collect();
            let rows = stmt.query_map(params.as_slice(), |row| {
                Ok((
                    row.get::<_, String>("path")?,
                    row.get::<_, i64>("id")?,
                    row.get::<_, String>("name")?,
                    row.get::<_, String>("source")?,
                ))
            })?;
            for row in rows {
                let (path, id, name, source) = row?;
                out.entry(path).or_default().push(Tag {
                    id,
                    name,
                    source: TagSource::from_str(&source)?,
                });
            }
        }
        Ok(out)
    }

    /// List every tag with its usage count, ordered by name.
    ///
    /// # Errors
    ///
    /// Returns [`DbError::Sqlite`] when the read fails. Returns
    /// [`DbError::Core`] when a stored tag source is unknown. Returns
    /// [`DbError::LockPoisoned`] after a panic poisoned the lock.
    pub fn all_tags(&self) -> Result<Vec<TagUsage>, DbError> {
        let conn = self.lock()?;
        let mut stmt = conn.prepare(
            "SELECT t.id AS id, t.name AS name, t.source AS source, \
             COUNT(ft.path) AS usage_count \
             FROM tags t LEFT JOIN file_tags ft ON ft.tag_id = t.id \
             GROUP BY t.id, t.name, t.source ORDER BY t.name",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, i64>("id")?,
                row.get::<_, String>("name")?,
                row.get::<_, String>("source")?,
                row.get::<_, i64>("usage_count")?,
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (id, name, source, usage_count) = row?;
            out.push(TagUsage {
                tag: Tag {
                    id,
                    name,
                    source: TagSource::from_str(&source)?,
                },
                usage_count: u32::try_from(usage_count).unwrap_or(0),
            });
        }
        Ok(out)
    }

    /// Remove one tag from many paths in one transaction.
    ///
    /// The path list is split into chunks of [`PATH_CHUNK`], so a large
    /// library cannot exceed the SQLite variable limit. The method returns
    /// the total number of deleted rows.
    ///
    /// # Errors
    ///
    /// Returns [`DbError::Sqlite`] when the statement fails. Returns
    /// [`DbError::LockPoisoned`] after a panic poisoned the lock.
    pub fn remove_tags_from_files(&self, paths: &[String], tag_id: i64) -> Result<usize, DbError> {
        if paths.is_empty() {
            return Ok(0);
        }
        let normalized: Vec<String> = paths.iter().map(|path| normalize_path(path)).collect();
        let mut conn = self.lock()?;
        let tx = conn.transaction()?;
        let mut changed = 0;
        for chunk in normalized.chunks(PATH_CHUNK) {
            let sql = format!(
                "DELETE FROM file_tags WHERE path IN ({}) AND tag_id = ?{}",
                placeholders(chunk.len()),
                chunk.len() + 1
            );
            let mut params: Vec<&dyn rusqlite::ToSql> = chunk
                .iter()
                .map(|path| path as &dyn rusqlite::ToSql)
                .collect();
            params.push(&tag_id);
            changed += tx.execute(&sql, params.as_slice())?;
        }
        tx.commit()?;
        Ok(changed)
    }

    /// Delete a user tag. The `file_tags` foreign key cascades the delete.
    ///
    /// Only user tags can be deleted. Color tags are made by the app. A
    /// missing id deletes nothing and returns zero.
    ///
    /// # Errors
    ///
    /// Returns [`DbError::ProtectedTag`] when the tag is not a user tag.
    /// Returns [`DbError::Core`] when the stored tag source is unknown.
    /// Returns [`DbError::Sqlite`] when the statement fails. Returns
    /// [`DbError::LockPoisoned`] after a panic poisoned the lock.
    pub fn delete_tag(&self, tag_id: i64) -> Result<usize, DbError> {
        let conn = self.lock()?;
        let found: Option<(String, String)> = conn
            .query_row(
                "SELECT name, source FROM tags WHERE id = ?1",
                [tag_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        let Some((name, source)) = found else {
            return Ok(0);
        };
        if TagSource::from_str(&source)? != TagSource::User {
            return Err(DbError::ProtectedTag(name));
        }
        let changed = conn.execute("DELETE FROM tags WHERE id = ?1", [tag_id])?;
        Ok(changed)
    }

    /// Rename a user tag and return the stored name.
    ///
    /// The name is trimmed and checked like any new user tag name. The tag id
    /// and its file associations stay. Names compare with case. A name that
    /// another tag uses is refused, and tags never merge. Renaming a tag to
    /// its own name writes nothing.
    ///
    /// # Errors
    ///
    /// Returns [`DbError::ReservedTagName`] for a reserved name. Returns
    /// [`DbError::InvalidTagName`] for an empty or too long name. Returns
    /// [`DbError::TagNotFound`] when no tag has the id. Returns
    /// [`DbError::ProtectedTag`] when the tag is not a user tag. Returns
    /// [`DbError::TagNameTaken`] when another tag uses the name. Returns
    /// [`DbError::Core`] when the stored tag source is unknown. Returns
    /// [`DbError::Sqlite`] when a statement fails. Returns
    /// [`DbError::LockPoisoned`] after a panic poisoned the lock.
    pub fn rename_tag(&self, tag_id: i64, new_name: &str) -> Result<String, DbError> {
        let name = validate_tag_name(new_name).map_err(|err| match err {
            TagNameError::Reserved => DbError::ReservedTagName(new_name.trim().to_owned()),
            other => DbError::InvalidTagName(other),
        })?;
        let conn = self.lock()?;
        let found: Option<(String, String)> = conn
            .query_row(
                "SELECT name, source FROM tags WHERE id = ?1",
                [tag_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        let Some((current, source)) = found else {
            return Err(DbError::TagNotFound(tag_id));
        };
        if TagSource::from_str(&source)? != TagSource::User {
            return Err(DbError::ProtectedTag(current));
        }
        if current == name {
            return Ok(name);
        }
        let taken = conn
            .query_row("SELECT 1 FROM tags WHERE name = ?1", [&name], |_| Ok(()))
            .optional()?
            .is_some();
        if taken {
            return Err(DbError::TagNameTaken(name));
        }
        match conn.execute(
            "UPDATE tags SET name = ?1 WHERE id = ?2",
            params![name, tag_id],
        ) {
            Ok(_) => Ok(name),
            // The read above already checked the name. This catches a
            // write from another connection that came first.
            Err(rusqlite::Error::SqliteFailure(err, _))
                if err.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_UNIQUE =>
            {
                Err(DbError::TagNameTaken(name))
            }
            Err(err) => Err(err.into()),
        }
    }

    /// Replace the `auto_color` associations of one path in one transaction.
    ///
    /// The method leaves user associations and other paths untouched. It
    /// returns the number of new association rows.
    ///
    /// # Errors
    ///
    /// Returns [`DbError::Sqlite`] when a statement fails. Returns
    /// [`DbError::LockPoisoned`] after a panic poisoned the lock.
    pub fn replace_auto_color_tags(&self, path: &str, names: &[String]) -> Result<usize, DbError> {
        let normalized = normalize_path(path);
        let mut conn = self.lock()?;
        let tx = conn.transaction()?;
        tx.execute(
            "DELETE FROM file_tags WHERE path = ?1 AND source = ?2",
            params![normalized, TagSource::AutoColor.as_str()],
        )?;
        let mut inserted = 0;
        for name in names {
            let tag = ensure_tag_conn(&tx, name, TagSource::AutoColor)?;
            inserted += tx.execute(
                "INSERT OR IGNORE INTO file_tags (path, tag_id, source) VALUES (?1, ?2, ?3)",
                params![normalized, tag.id, TagSource::AutoColor.as_str()],
            )?;
        }
        tx.commit()?;
        Ok(inserted)
    }

    /// Return every path that has at least one `auto_color` association.
    ///
    /// Paths are returned as stored: normalized, with `/` separators. The set
    /// can hold one entry per library file, so call this off the UI thread.
    ///
    /// # Errors
    ///
    /// Returns [`DbError::Sqlite`] when the statement fails. Returns
    /// [`DbError::LockPoisoned`] after a panic poisoned the lock.
    pub fn auto_colored_paths(&self) -> Result<HashSet<String>, DbError> {
        let conn = self.lock()?;
        let mut stmt = conn.prepare("SELECT DISTINCT path FROM file_tags WHERE source = ?1")?;
        let paths = stmt
            .query_map(params![TagSource::AutoColor.as_str()], |row| row.get(0))?
            .collect::<Result<HashSet<String>, _>>()?;
        Ok(paths)
    }
}
