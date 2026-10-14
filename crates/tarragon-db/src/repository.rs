//! Repository methods for the eight stored tables.
//!
//! Paths are stored with forward slashes on every platform, as the Python app
//! does. Reads and writes run on the shared connection. Every batched write
//! uses one transaction.

use std::collections::HashMap;
use std::path::Path;
use std::str::FromStr;

use rusqlite::{Connection, OptionalExtension, Row, params};
use tarragon_core::entity::{FileInfo, Tag, TagSource};

use crate::{Database, DbError};

/// A thumbnail row read from the database.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Thumbnail {
    /// Stored file path. It uses forward slashes.
    pub path: String,
    /// Source last-modified time in epoch seconds.
    pub mtime: i64,
    /// Source file size in bytes.
    pub size: i64,
    /// Unused legacy hash from the Python schema.
    pub thumb_hash: Option<String>,
    /// Pixel width. Zero means unknown.
    pub width: u32,
    /// Pixel height. Zero means unknown.
    pub height: u32,
    /// Row creation time text from `datetime('now')`.
    pub created_at: String,
    /// Cache folder UUID for the source folder.
    pub cache_uuid: Option<String>,
    /// Path of the 256 px tier file.
    pub thumbnail_cache_path: Option<String>,
    /// Path of the 1024 px tier file.
    pub preview_cache_path: Option<String>,
    /// Path of the full tier file.
    pub full_cache_path: Option<String>,
    /// Last cache access in epoch seconds. It orders the Q33 LRU eviction.
    pub last_accessed: i64,
}

/// A thumbnail row written after a render completes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThumbnailWrite {
    /// Stored file path. It uses forward slashes.
    pub path: String,
    /// Source last-modified time in epoch seconds.
    pub mtime: i64,
    /// Source file size in bytes.
    pub size: i64,
    /// Pixel width.
    pub width: u32,
    /// Pixel height.
    pub height: u32,
    /// Cache folder UUID for the source folder.
    pub cache_uuid: String,
    /// Path of the 256 px tier file.
    pub thumbnail_cache_path: Option<String>,
    /// Path of the 1024 px tier file.
    pub preview_cache_path: Option<String>,
    /// Path of the full tier file.
    pub full_cache_path: Option<String>,
    /// Last cache access in epoch seconds.
    pub last_accessed: i64,
}

/// A favorite row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Favorite {
    /// Database row id.
    pub id: i64,
    /// Stored file path. It uses forward slashes.
    pub path: String,
    /// Optional display label.
    pub label: Option<String>,
    /// Display order. A lower number comes first.
    pub sort_order: i64,
}

/// A tag and its file-association count.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TagUsage {
    /// The tag row.
    pub tag: Tag,
    /// Number of `file_tags` rows for the tag.
    pub usage_count: u32,
}

/// The upsert for one scan row. A conflict keeps the stored `mtime` and
/// `size` so a later render can detect a source change.
const SCAN_STUB_SQL: &str = "\
INSERT INTO thumbnails (path, mtime, size, width, height, cache_uuid, thumbnail_cache_path, preview_cache_path, full_cache_path, last_accessed)
VALUES (?1, ?2, ?3, ?4, ?5, '', NULL, NULL, NULL, 0)
ON CONFLICT(path) DO UPDATE SET mtime = thumbnails.mtime, size = thumbnails.size";

/// The maximum number of bound paths in one `IN (...)` list.
///
/// The chunk keeps the bind count well below the SQLite variable limit
/// (999 on old builds, 32766 on current builds).
const PATH_CHUNK: usize = 500;

/// The column list for a thumbnail read.
const THUMBNAIL_COLUMNS: &str = "\
path, mtime, size, thumb_hash, width, height, created_at, cache_uuid, \
thumbnail_cache_path, preview_cache_path, full_cache_path, last_accessed";

/// Return `path` with backslashes changed to forward slashes.
fn normalize_path(path: &str) -> String {
    path.replace('\\', "/")
}

/// Escape the LIKE wildcards `%`, `_`, and `\` so they match literally.
fn escape_like(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

/// Return the LIKE pattern that matches a folder and its direct children.
fn folder_like(folder: &str) -> String {
    let normalized = normalize_path(folder);
    let trimmed = normalized.trim_end_matches('/');
    format!("{}/%", escape_like(trimmed))
}

/// Return the LIKE pattern that matches the deeper descendants of a folder.
///
/// SQLite `%` also matches `/`. The extra `/%` tail therefore matches a path
/// with one more separator, which is a nested child and never a direct child.
fn folder_deep_like(folder: &str) -> String {
    let normalized = normalize_path(folder);
    let trimmed = normalized.trim_end_matches('/');
    format!("{}/%/%", escape_like(trimmed))
}

/// Map a thumbnail row.
fn row_to_thumbnail(row: &Row<'_>) -> rusqlite::Result<Thumbnail> {
    Ok(Thumbnail {
        path: row.get("path")?,
        mtime: row.get("mtime")?,
        size: row.get("size")?,
        thumb_hash: row.get("thumb_hash")?,
        width: row.get("width")?,
        height: row.get("height")?,
        created_at: row.get("created_at")?,
        cache_uuid: row.get("cache_uuid")?,
        thumbnail_cache_path: row.get("thumbnail_cache_path")?,
        preview_cache_path: row.get("preview_cache_path")?,
        full_cache_path: row.get("full_cache_path")?,
        last_accessed: row.get("last_accessed")?,
    })
}

/// Map a favorite row.
fn row_to_favorite(row: &Row<'_>) -> rusqlite::Result<Favorite> {
    Ok(Favorite {
        id: row.get("id")?,
        path: row.get("path")?,
        label: row.get("label")?,
        sort_order: row.get("sort_order")?,
    })
}

/// Insert a tag if absent and return it with the requested source.
fn ensure_tag_conn(conn: &Connection, name: &str, source: TagSource) -> Result<Tag, DbError> {
    let id = conn.query_row(
        "INSERT INTO tags (name, source) VALUES (?1, ?2) \
         ON CONFLICT(name) DO UPDATE SET name = name RETURNING id",
        params![name, source.as_str()],
        |row| row.get::<_, i64>("id"),
    )?;
    Ok(Tag {
        id,
        name: name.to_owned(),
        source,
    })
}

/// Build a parenthesized `?` placeholder list for `count` values.
fn placeholders(count: usize) -> String {
    let mut text = String::with_capacity(count * 3);
    for index in 0..count {
        if index > 0 {
            text.push_str(", ");
        }
        text.push('?');
    }
    text
}

impl Database {
    // -----------------------------------------------------------------------
    // thumbnails
    // -----------------------------------------------------------------------

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
    /// This is the batched LRU write from Q33. The method returns the number
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

    /// Delete every thumbnail row. The cache purge calls this (Q25).
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

    // -----------------------------------------------------------------------
    // tags and file_tags
    // -----------------------------------------------------------------------

    /// Insert a tag if absent and return it.
    ///
    /// The stored `source` never changes on conflict. The returned tag keeps
    /// the requested `source`, which callers use for the association row.
    ///
    /// # Errors
    ///
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

    /// Delete a tag. The `file_tags` foreign key cascades the delete (Q21).
    ///
    /// # Errors
    ///
    /// Returns [`DbError::Sqlite`] when the statement fails. Returns
    /// [`DbError::LockPoisoned`] after a panic poisoned the lock.
    pub fn delete_tag(&self, tag_id: i64) -> Result<usize, DbError> {
        let conn = self.lock()?;
        let changed = conn.execute("DELETE FROM tags WHERE id = ?1", [tag_id])?;
        Ok(changed)
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

    // -----------------------------------------------------------------------
    // favorites
    // -----------------------------------------------------------------------

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

    // -----------------------------------------------------------------------
    // settings
    // -----------------------------------------------------------------------

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

    // -----------------------------------------------------------------------
    // editor_associations
    // -----------------------------------------------------------------------

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

    // -----------------------------------------------------------------------
    // folder_cache_uuids
    // -----------------------------------------------------------------------

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
        let folders: Vec<String> = {
            let conn = self.lock()?;
            let mut stmt = conn.prepare("SELECT folder_path FROM folder_cache_uuids")?;
            let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
            rows.collect::<rusqlite::Result<Vec<_>>>()?
        };
        let stale: Vec<String> = folders
            .into_iter()
            .filter(|folder| !Path::new(folder).is_dir())
            .collect();
        if stale.is_empty() {
            return Ok(0);
        }
        let mut conn = self.lock()?;
        let tx = conn.transaction()?;
        let mut removed = 0;
        {
            let mut stmt = tx.prepare("DELETE FROM folder_cache_uuids WHERE folder_path = ?1")?;
            for folder in &stale {
                removed += stmt.execute([folder])?;
            }
        }
        tx.commit()?;
        Ok(removed)
    }
}
