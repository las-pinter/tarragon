//! Repository methods for the eight stored tables.
//!
//! Paths are stored with forward slashes on every platform. Reads and writes
//! run on the shared connection. Every batched write uses one transaction.

use rusqlite::{Connection, Row, params};
use tarragon_core::entity::{Tag, TagSource};

use crate::DbError;

mod editor;
mod favorites;
mod folder;
mod settings;
mod tags;
mod thumbnail;

/// A thumbnail row read from the database.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Thumbnail {
    /// Stored file path. It uses forward slashes.
    pub path: String,
    /// Source last-modified time in epoch seconds.
    pub mtime: i64,
    /// Source file size in bytes.
    pub size: i64,
    /// Unused legacy hash column.
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
    /// Last cache access in epoch seconds. It orders the LRU eviction.
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

/// The maximum number of bound paths in one `IN (...)` list.
///
/// The chunk keeps the bind count well below the SQLite variable limit
/// (999 on old builds, 32766 on current builds).
const PATH_CHUNK: usize = 500;

/// The column list for a thumbnail read.
pub(crate) const THUMBNAIL_COLUMNS: &str = "\
path, mtime, size, thumb_hash, width, height, created_at, cache_uuid, \
thumbnail_cache_path, preview_cache_path, full_cache_path, last_accessed";

/// Map a thumbnail row.
pub(crate) fn row_to_thumbnail(row: &Row<'_>) -> rusqlite::Result<Thumbnail> {
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
