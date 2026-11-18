//! Shared decode limits for every renderer.
//!
//! The `image` crate checks the dimension limits at the header, but
//! `DynamicImage::from_decoder` allocates `total_bytes()` with no allocation
//! check. This module applies both checks before the allocation, so a header
//! that claims a huge canvas becomes an error instead of an aborted process.

use std::io::{BufRead, Seek};
use std::path::Path;

use image::{DynamicImage, ImageDecoder, ImageReader, Limits};
use tarragon_core::error::RenderError;

use crate::renderer::RenderedImage;

/// The decoder allocation cap, in bytes.
///
/// An 8-bit RGBA canvas at the 8192 edge needs 256 MiB. A 16-bit RGBA canvas
/// at the same edge needs 512 MiB, which equals the cap exactly. The strict
/// greater-than check lets that canvas pass.
pub(crate) const MAX_DECODE_ALLOC_BYTES: u64 = 512 * 1024 * 1024;
/// The decoder allocation cap for plain image files, in bytes.
///
/// A plain file is the artist's own reference, and a large valid one such as
/// a 16-bit TIFF must still open. A document preview is small by design, so
/// it keeps the lower cap.
pub(crate) const MAX_PLAIN_ALLOC_BYTES: u64 = 1024 * 1024 * 1024;
/// The edge cap for documents that embed a preview or a merged canvas.
///
/// This is the full-tier edge.
pub(crate) const MAX_DOCUMENT_EDGE: u32 = 8192;
/// The edge cap for plain image files.
///
/// A source can be larger than the full tier. The byte cap is the bound that
/// prevents the abort; this cap only stops absurd headers early.
pub(crate) const MAX_PLAIN_EDGE: u32 = 32_768;

/// Decode with the size limits applied, then apply the EXIF orientation.
///
/// The reader must have its format set or guessed. The edge cap bounds each
/// side and `max_bytes` bounds the decoded buffer.
pub(crate) fn decode_with_limits<R: BufRead + Seek>(
    path: &Path,
    mut reader: ImageReader<R>,
    max_edge: u32,
    max_bytes: u64,
) -> Result<RenderedImage, RenderError> {
    let mut limits = Limits::default();
    // Assigning the documented default pins it against an upstream change.
    limits.max_alloc = Some(max_bytes);
    limits.max_image_width = Some(max_edge);
    limits.max_image_height = Some(max_edge);
    reader.limits(limits);
    let mut decoder = reader
        .into_decoder()
        .map_err(|source| decode_error(path, &source))?;
    // `from_decoder` allocates without a limit check, so check the size here.
    let total_bytes = decoder.total_bytes();
    if total_bytes > max_bytes {
        return Err(RenderError::Decode {
            path: path.to_path_buf(),
            reason: format!("the canvas needs {total_bytes} bytes, above the {max_bytes} byte cap"),
        });
    }
    // `decode()` does not apply orientation, so read the tag first.
    let orientation = decoder
        .orientation()
        .map_err(|source| decode_error(path, &source))?;
    let mut image =
        DynamicImage::from_decoder(decoder).map_err(|source| decode_error(path, &source))?;
    image.apply_orientation(orientation);
    Ok(image)
}

/// Build a decode error from an image error.
fn decode_error(path: &Path, source: &image::ImageError) -> RenderError {
    RenderError::Decode {
        path: path.to_path_buf(),
        reason: source.to_string(),
    }
}
