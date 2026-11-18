//! Renderer for plain image formats.
//!
//! The plain renderer decodes JPEG, PNG, WebP, and TIFF through the `image`
//! crate. It returns a `DynamicImage` with the EXIF orientation applied: the
//! pixels the cache store and the UI consume. The registry uses this renderer
//! as the default fallback.

#![allow(clippy::module_name_repetitions)]

use std::io::BufReader;
use std::path::Path;

use image::ImageReader;
use tarragon_core::error::RenderError;

use crate::limits::{MAX_PLAIN_ALLOC_BYTES, MAX_PLAIN_EDGE, decode_with_limits};
use crate::renderer::RenderedImage;

/// Decode `path` to a `DynamicImage` with the EXIF orientation applied.
///
/// The format is guessed from the file content, so a wrong extension does not
/// hide the real format. The registry validates the extension first. The
/// orientation step applies the EXIF orientation. The decode refuses a canvas
/// above the shared size limits instead of allocating it.
///
/// # Errors
///
/// Returns [`RenderError::Io`] when the file cannot be read. Returns
/// [`RenderError::Decode`] when the bytes are not a decodable image.
pub fn render_plain_image(path: &Path) -> Result<RenderedImage, RenderError> {
    let file = std::fs::File::open(path).map_err(|source| RenderError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let reader = ImageReader::new(BufReader::new(file))
        .with_guessed_format()
        .map_err(|source| RenderError::Io {
            path: path.to_path_buf(),
            source,
        })?;
    decode_with_limits(path, reader, MAX_PLAIN_EDGE, MAX_PLAIN_ALLOC_BYTES)
}
