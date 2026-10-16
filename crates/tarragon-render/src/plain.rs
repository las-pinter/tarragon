//! Renderer for plain image formats.
//!
//! The plain renderer decodes JPEG, PNG, WebP, and TIFF through the `image`
//! crate. It returns a `DynamicImage` with the EXIF orientation applied: the
//! pixels the cache store and the UI consume. The registry uses this renderer
//! as the default fallback (task 16).

#![allow(clippy::module_name_repetitions)]

use std::io::BufReader;
use std::path::Path;

use image::{DynamicImage, ImageDecoder, ImageReader};
use tarragon_core::error::RenderError;

use crate::renderer::RenderedImage;

/// Decode `path` to a `DynamicImage` with the EXIF orientation applied.
///
/// The format is guessed from the file content, so a wrong extension does not
/// hide the real format. The registry validates the extension first. The
/// orientation step matches Python `ImageOps.exif_transpose` in
/// `renderers/plain.py`.
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
    let mut decoder = ImageReader::new(BufReader::new(file))
        .with_guessed_format()
        .map_err(|source| RenderError::Decode {
            path: path.to_path_buf(),
            reason: source.to_string(),
        })?
        .into_decoder()
        .map_err(|source| RenderError::Decode {
            path: path.to_path_buf(),
            reason: source.to_string(),
        })?;
    // `decode()` does not apply orientation, so read the tag first.
    let orientation = decoder
        .orientation()
        .map_err(|source| RenderError::Decode {
            path: path.to_path_buf(),
            reason: source.to_string(),
        })?;
    let mut image = DynamicImage::from_decoder(decoder).map_err(|source| RenderError::Decode {
        path: path.to_path_buf(),
        reason: source.to_string(),
    })?;
    image.apply_orientation(orientation);
    Ok(image)
}
