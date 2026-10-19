//! CLIP renderer behavior.
//!
//! The tests prove the behavior: the committed `.clip` fixture renders its
//! embedded PNG preview, the renderer reads only the bytes it needs, a header
//! that straddles a scan chunk boundary is still found, and a malformed or
//! missing file takes the typed error path instead of a panic.
//!
//! The fixture lives in `tests/fixtures/clip/`. Its builder is in
//! `tests/clip_fixture/`, and the check test keeps the committed bytes and the
//! builder in step.

mod clip_fixture;

use std::io::{Cursor, Read, Seek, SeekFrom};

use image::{DynamicImage, ImageFormat};
use tarragon_core::error::RenderError;
use tarragon_render::clip::{render_clip_from_source, render_clip_image};
use tarragon_render::registry;

use clip_fixture as fixture;

/// A reader that counts the bytes it hands out.
struct CountingReader {
    inner: Cursor<Vec<u8>>,
    bytes_read: u64,
}

impl CountingReader {
    /// Wrap the given bytes.
    fn new(bytes: Vec<u8>) -> Self {
        Self {
            inner: Cursor::new(bytes),
            bytes_read: 0,
        }
    }
}

impl Read for CountingReader {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        let read = self.inner.read(buffer)?;
        self.bytes_read += read as u64;
        Ok(read)
    }
}

impl Seek for CountingReader {
    fn seek(&mut self, position: SeekFrom) -> std::io::Result<u64> {
        self.inner.seek(position)
    }
}

/// Encode a small PNG that fits on one database page.
fn small_png() -> Vec<u8> {
    let mut bytes = Vec::new();
    DynamicImage::new_rgba8(5, 4)
        .write_to(&mut Cursor::new(&mut bytes), ImageFormat::Png)
        .expect("the small png encodes");
    bytes
}

/// Write bytes to a file in a temporary folder and return the path.
fn write_temp(folder: &fixture::TempDir, name: &str, bytes: &[u8]) -> std::path::PathBuf {
    let path = folder.path().join(name);
    std::fs::write(&path, bytes).expect("the file is written");
    path
}

#[test]
fn the_committed_fixture_matches_the_builder() {
    let committed = std::fs::read(fixture::fixture("preview.clip")).expect("the fixture reads");
    let built = fixture::build_default();
    assert_eq!(committed, built, "the committed fixture drifted");
    let png_len = fixture::preview_png().len();
    assert!(
        png_len + 8 > 4096 - 35,
        "the preview must spill onto overflow pages: {png_len} bytes"
    );
    assert!(
        committed.len() > 3 * 4096,
        "the fixture must hold overflow pages: {} bytes",
        committed.len()
    );
}

#[test]
#[ignore = "rewrites the committed clip fixture"]
fn rewrite_the_committed_fixture() {
    std::fs::write(fixture::fixture("preview.clip"), fixture::build_default())
        .expect("the fixture is written");
}

#[test]
fn the_committed_fixture_renders_its_preview() {
    let rendered =
        registry::render(&fixture::fixture("preview.clip")).expect("the fixture renders");
    assert_eq!(
        (rendered.width(), rendered.height()),
        (fixture::PREVIEW_WIDTH, fixture::PREVIEW_HEIGHT)
    );
    assert_eq!(rendered.color(), image::ColorType::Rgba8);
    let pixels = rendered.to_rgba8();
    let expected = fixture::preview_image();
    // PNG is lossless, so every pixel must come back unchanged.
    assert_eq!(pixels.as_raw(), expected.as_raw());
    // A partly transparent pixel and an opaque pixel, named for the report.
    assert_eq!(pixels.get_pixel(0, 0).0, [0, 0, 0, 128]);
    assert_eq!(pixels.get_pixel(1, 1).0, [8, 18, 30, 255]);
}

#[test]
fn a_small_preview_without_overflow_renders() {
    let folder = fixture::TempDir::new("small");
    let path = write_temp(
        &folder,
        "small.clip",
        &fixture::build_clip(&small_png(), 0, 0),
    );
    let rendered = render_clip_image(&path).expect("the small preview renders");
    assert_eq!((rendered.width(), rendered.height()), (5, 4));
}

#[test]
fn a_header_across_a_scan_chunk_boundary_is_found() {
    // Put the SQLite header ten bytes before the end of the first scan chunk.
    let prefix_junk = 256 * 1024 - 10 - 136;
    let folder = fixture::TempDir::new("straddle");
    let path = write_temp(
        &folder,
        "straddle.clip",
        &fixture::build_clip(&fixture::preview_png(), prefix_junk, 0),
    );
    let rendered = render_clip_image(&path).expect("the late preview renders");
    assert_eq!(
        (rendered.width(), rendered.height()),
        (fixture::PREVIEW_WIDTH, fixture::PREVIEW_HEIGHT)
    );
}

#[test]
fn a_two_level_tree_renders_a_two_page_overflow_preview() {
    // The padding after the PNG fills the blob to a length that needs exactly
    // two overflow pages. The renderer cuts the PNG at its end chunk.
    let mut blob = fixture::preview_png();
    blob.resize(10_000, 0xa5);
    assert_eq!(fixture::overflow_pages_for(blob.len()), 2);
    let bytes = fixture::build_two_level(&blob);
    let mut reader = Cursor::new(bytes);
    let rendered = render_clip_from_source(&mut reader, std::path::Path::new("two-level.clip"))
        .expect("the two-level preview renders");
    assert_eq!(
        (rendered.width(), rendered.height()),
        (fixture::PREVIEW_WIDTH, fixture::PREVIEW_HEIGHT)
    );
    assert_eq!(rendered.color(), image::ColorType::Rgba8);
    assert_eq!(
        rendered.to_rgba8().as_raw(),
        fixture::preview_image().as_raw()
    );
}

#[test]
fn the_whole_file_is_not_read() {
    // A four-megabyte payload follows the last database page.
    let trailing = 4 * 1024 * 1024;
    let bytes = fixture::build_clip(&fixture::preview_png(), 0, trailing);
    let total = bytes.len();
    let mut reader = CountingReader::new(bytes);
    let rendered =
        render_clip_from_source(&mut reader, std::path::Path::new("stream.clip")).expect("renders");
    assert_eq!(
        (rendered.width(), rendered.height()),
        (fixture::PREVIEW_WIDTH, fixture::PREVIEW_HEIGHT)
    );
    assert!(
        reader.bytes_read < 1024 * 1024,
        "the renderer read {} of {total} bytes",
        reader.bytes_read
    );
}

#[test]
fn a_garbage_file_takes_the_error_path() {
    let folder = fixture::TempDir::new("garbage");
    let path = write_temp(&folder, "garbage.clip", b"no database lives here");
    let result = render_clip_image(&path);
    assert!(
        matches!(result, Err(RenderError::Decode { .. })),
        "unexpected result: {result:?}"
    );
}

#[test]
fn a_truncated_database_takes_the_error_path() {
    let folder = fixture::TempDir::new("truncated");
    let mut bytes = fixture::build_default();
    bytes.truncate(136 + 4096 + 32);
    let path = write_temp(&folder, "truncated.clip", &bytes);
    let result = render_clip_image(&path);
    assert!(
        matches!(result, Err(RenderError::Decode { .. })),
        "unexpected result: {result:?}"
    );
}

#[test]
fn a_database_without_the_preview_table_takes_the_error_path() {
    let folder = fixture::TempDir::new("no-table");
    let bytes = fixture::build_with_table("Other", &fixture::preview_png());
    let path = write_temp(&folder, "no-table.clip", &bytes);
    let result = render_clip_image(&path);
    assert!(
        matches!(result, Err(RenderError::Decode { .. })),
        "unexpected result: {result:?}"
    );
}

#[test]
fn a_null_preview_takes_the_error_path() {
    let folder = fixture::TempDir::new("null");
    let path = write_temp(&folder, "null.clip", &fixture::build_with_null());
    let result = render_clip_image(&path);
    assert!(
        matches!(result, Err(RenderError::Decode { .. })),
        "unexpected result: {result:?}"
    );
}

#[test]
fn a_different_column_name_takes_the_error_path() {
    let folder = fixture::TempDir::new("column");
    let bytes = fixture::build_with_column("Other", &fixture::preview_png());
    let path = write_temp(&folder, "column.clip", &bytes);
    let result = render_clip_image(&path);
    assert!(
        matches!(result, Err(RenderError::Decode { .. })),
        "unexpected result: {result:?}"
    );
}

#[test]
fn a_blob_without_a_png_takes_the_error_path() {
    let folder = fixture::TempDir::new("no-png");
    let bytes = fixture::build_clip(b"\xff\xd8\xff\xe0 not a png", 0, 0);
    let path = write_temp(&folder, "no-png.clip", &bytes);
    let result = render_clip_image(&path);
    assert!(
        matches!(result, Err(RenderError::Decode { .. })),
        "unexpected result: {result:?}"
    );
}

#[test]
fn a_missing_file_takes_the_error_path() {
    let folder = fixture::TempDir::new("missing");
    let result = render_clip_image(&folder.path().join("gone.clip"));
    assert!(
        matches!(result, Err(RenderError::Io { .. })),
        "unexpected result: {result:?}"
    );
}
