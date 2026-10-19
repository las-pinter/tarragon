//! Renderer for Clip Studio Paint `.clip` files.
//!
//! A `.clip` document holds a SQLite database after a binary header. The
//! database has a `CanvasPreview` table whose `ImageData` column carries a PNG
//! preview. The renderer scans the file for the SQLite header in fixed-size
//! chunks, reads only the database pages it needs, and decodes the preview.
//! The whole document never enters memory.
//!
//! A file with no embeddable preview takes the typed error path: the caller
//! shows the error card. Streamed reads keep memory bounded for multi-gigabyte
//! documents.

#![allow(clippy::module_name_repetitions)]

mod record;
mod sqlite;

use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use image::{ColorType, DynamicImage, ImageFormat};
use tarragon_core::error::RenderError;

use self::record::{Value, column_index, parse_record};
use self::sqlite::{Database, DbError};
use crate::renderer::RenderedImage;

/// The SQLite header magic that marks the embedded database.
const SQLITE_HEADER: &[u8] = b"SQLite format 3\0";
/// The scan chunk size. Memory stays at one chunk regardless of file size.
const SCAN_CHUNK_SIZE: usize = 256 * 1024;
/// The PNG signature that starts the preview image.
const PNG_SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";
/// The PNG end chunk marker.
const PNG_IEND: &[u8] = b"IEND";
/// The four CRC bytes that follow the `IEND` marker.
const PNG_IEND_CRC_BYTES: usize = 4;
/// The table that holds the preview row.
const PREVIEW_TABLE: &str = "CanvasPreview";
/// The column that holds the PNG blob.
const PREVIEW_COLUMN: &str = "ImageData";

/// Render a `.clip` file to a `DynamicImage`.
///
/// The renderer finds the embedded SQLite database, queries the first
/// `CanvasPreview` row, extracts the PNG from the `ImageData` blob, and decodes
/// it. The preview is the full resolution the document stores; the cache store
/// owns the tier scaling.
///
/// # Errors
///
/// Returns [`RenderError::Io`] when the file cannot be opened or read. Returns
/// [`RenderError::Decode`] when the file holds no SQLite database, no preview
/// row, no PNG, or a PNG that does not decode.
pub fn render_clip_image(path: &Path) -> Result<RenderedImage, RenderError> {
    let mut file = std::fs::File::open(path).map_err(|source| RenderError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    render_clip_from_source(&mut file, path)
}

/// Render a `.clip` preview from an open stream.
///
/// The stream must start at the document start. The renderer seeks within it
/// and reads only the chunks and pages it needs, so the caller can pass a file
/// handle and keep memory bounded.
///
/// # Errors
///
/// Returns [`RenderError::Io`] when the stream cannot be read. Returns
/// [`RenderError::Decode`] when the bytes hold no decodable preview.
pub fn render_clip_from_source<R: Read + Seek>(
    source: &mut R,
    path: &Path,
) -> Result<RenderedImage, RenderError> {
    let offset = find_sqlite_header(source).map_err(|source_error| RenderError::Io {
        path: path.to_path_buf(),
        source: source_error,
    })?;
    let Some(offset) = offset else {
        return Err(decode_error(
            path,
            "the file holds no embedded SQLite database",
        ));
    };
    let mut database = Database::open(source, offset).map_err(|error| map_db_error(path, error))?;
    let blob = preview_blob(&mut database).map_err(|error| map_db_error(path, error))?;
    let png = extract_png(&blob)
        .ok_or_else(|| decode_error(path, "the preview blob holds no complete PNG"))?;
    decode_png(path, png)
}

/// Read the first `CanvasPreview.ImageData` blob from the database.
fn preview_blob<R: Read + Seek>(database: &mut Database<R>) -> Result<Vec<u8>, DbError> {
    let Some(table) = database.lookup_table(PREVIEW_TABLE)? else {
        return Err(DbError::Format(format!(
            "the database has no {PREVIEW_TABLE} table"
        )));
    };
    let index = column_index(&table.sql, PREVIEW_COLUMN).ok_or_else(|| {
        DbError::Format(format!("{PREVIEW_TABLE} has no {PREVIEW_COLUMN} column"))
    })?;
    let Some(payload) = database.first_row(table.root_page)? else {
        return Err(DbError::Format(format!("{PREVIEW_TABLE} has no row")));
    };
    let values = parse_record(&payload)?;
    match values.get(index) {
        Some(Value::Blob(bytes)) => Ok(bytes.to_vec()),
        Some(Value::Null) => Err(DbError::Format(format!(
            "{PREVIEW_TABLE}.{PREVIEW_COLUMN} is NULL"
        ))),
        Some(Value::Real(value)) => Err(DbError::Format(format!(
            "{PREVIEW_TABLE}.{PREVIEW_COLUMN} is the real value {value}, not a blob"
        ))),
        Some(Value::Integer(value)) => Err(DbError::Format(format!(
            "{PREVIEW_TABLE}.{PREVIEW_COLUMN} is the integer {value}, not a blob"
        ))),
        Some(Value::Text(_)) => Err(DbError::Format(format!(
            "{PREVIEW_TABLE}.{PREVIEW_COLUMN} is text, not a blob"
        ))),
        None => Err(DbError::Format(format!(
            "the {PREVIEW_TABLE} row has no {PREVIEW_COLUMN} value"
        ))),
    }
}

/// Extract the PNG that the blob carries between its signature and its end.
fn extract_png(blob: &[u8]) -> Option<&[u8]> {
    let start = find_subslice(blob, PNG_SIGNATURE)?;
    let end =
        find_subslice(&blob[start..], PNG_IEND)? + start + PNG_IEND.len() + PNG_IEND_CRC_BYTES;
    blob.get(start..end)
}

/// Decode the PNG bytes and keep an 8-bit RGB or RGBA image as it is.
fn decode_png(path: &Path, png: &[u8]) -> Result<RenderedImage, RenderError> {
    let image = image::load_from_memory_with_format(png, ImageFormat::Png).map_err(|source| {
        RenderError::Decode {
            path: path.to_path_buf(),
            reason: source.to_string(),
        }
    })?;
    Ok(match image.color() {
        ColorType::Rgb8 | ColorType::Rgba8 => image,
        _ => DynamicImage::ImageRgba8(image.to_rgba8()),
    })
}

/// Scan the stream in chunks and return the offset of the SQLite header.
///
/// The scan keeps a tail overlap of `SQLITE_HEADER.len() - 1` bytes, so a
/// header that straddles a chunk boundary is still found. Memory stays at one
/// chunk. On success the stream sits at the header offset.
fn find_sqlite_header<R: Read + Seek>(source: &mut R) -> std::io::Result<Option<u64>> {
    source.seek(SeekFrom::Start(0))?;
    let mut chunk = vec![0_u8; SCAN_CHUNK_SIZE];
    let mut tail: Vec<u8> = Vec::new();
    let mut consumed = 0_u64;
    loop {
        let read = source.read(&mut chunk)?;
        if read == 0 {
            return Ok(None);
        }
        let mut window = Vec::with_capacity(tail.len() + read);
        window.extend_from_slice(&tail);
        window.extend_from_slice(&chunk[..read]);
        if let Some(position) = find_subslice(&window, SQLITE_HEADER) {
            let start = consumed - tail.len() as u64 + position as u64;
            source.seek(SeekFrom::Start(start))?;
            return Ok(Some(start));
        }
        let keep = SQLITE_HEADER.len() - 1;
        tail = window[window.len().saturating_sub(keep)..].to_vec();
        consumed += read as u64;
    }
}

/// Return the first position of `needle` in `haystack`.
fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

/// Build a decode error with a reason.
fn decode_error(path: &Path, reason: &str) -> RenderError {
    RenderError::Decode {
        path: path.to_path_buf(),
        reason: reason.to_owned(),
    }
}

/// Map a database reader error onto the render error type.
fn map_db_error(path: &Path, error: DbError) -> RenderError {
    match error {
        DbError::Io(source) => RenderError::Io {
            path: path.to_path_buf(),
            source,
        },
        DbError::Format(reason) => decode_error(path, &reason),
    }
}

#[cfg(test)]
mod tests {
    use super::{extract_png, find_subslice};

    #[test]
    fn the_png_is_cut_at_the_end_chunk() {
        let mut blob = b"junk".to_vec();
        let png = b"\x89PNG\r\n\x1a\nbodyIEND\x00\x00\x00\x00trailing";
        blob.extend_from_slice(png);
        blob.extend_from_slice(b"more junk");
        let extracted = extract_png(&blob).expect("the png is found");
        assert_eq!(extracted, &png[..png.len() - b"trailing".len()]);
    }

    #[test]
    fn a_blob_without_a_png_yields_nothing() {
        assert!(extract_png(b"no image here").is_none());
    }

    #[test]
    fn a_blob_with_no_end_chunk_yields_nothing() {
        assert!(extract_png(b"\x89PNG\r\n\x1a\nbody").is_none());
    }

    #[test]
    fn the_subslice_search_finds_and_misses() {
        assert_eq!(find_subslice(b"hello world", b"world"), Some(6));
        assert_eq!(find_subslice(b"hello", b"world"), None);
        assert_eq!(find_subslice(b"hello", b""), None);
    }
}
