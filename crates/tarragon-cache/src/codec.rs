//! Image scaling and JPEG encoding for the tier files.
//!
//! Each tier is scaled to its own longest edge and is never upscaled. Alpha
//! flattens onto white `(255, 255, 255)`. Every tier file is written as JPEG
//! q90.

use image::codecs::jpeg::JpegEncoder;
use image::imageops::FilterType;
use image::{DynamicImage, Rgb, RgbImage};

use crate::CacheError;

/// The JPEG quality. Fixed.
pub const JPEG_QUALITY: u8 = 90;

/// Scale `source` to fit within `edge` on the longest side.
///
/// A source that already fits is cloned unchanged, so the cache never
/// upscales.
pub(crate) fn fit_to_edge(source: &DynamicImage, edge: u32) -> DynamicImage {
    if source.width() <= edge && source.height() <= edge {
        source.clone()
    } else {
        source.resize(edge, edge, FilterType::Lanczos3)
    }
}

/// Flatten alpha onto white.
pub(crate) fn flatten_onto_white(source: &DynamicImage) -> RgbImage {
    let rgba = source.to_rgba8();
    let mut out = RgbImage::new(rgba.width(), rgba.height());
    let blend = |channel: u8, alpha: u32| -> u8 {
        let value = (u32::from(channel) * alpha + 255 * (255 - alpha) + 127) / 255;
        u8::try_from(value).unwrap_or(u8::MAX)
    };
    for (x, y, pixel) in rgba.enumerate_pixels() {
        let alpha = u32::from(pixel[3]);
        out.put_pixel(
            x,
            y,
            Rgb([
                blend(pixel[0], alpha),
                blend(pixel[1], alpha),
                blend(pixel[2], alpha),
            ]),
        );
    }
    out
}

/// Encode an RGB image as JPEG at [`JPEG_QUALITY`].
///
/// # Errors
///
/// Returns [`CacheError::Encode`] when the encoder fails.
pub(crate) fn encode_jpeg(image: &RgbImage) -> Result<Vec<u8>, CacheError> {
    let mut bytes = Vec::new();
    let mut encoder = JpegEncoder::new_with_quality(&mut bytes, JPEG_QUALITY);
    encoder.encode_image(image).map_err(CacheError::Encode)?;
    Ok(bytes)
}
