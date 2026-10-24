//! Image scaling and JPEG encoding for the tier files.
//!
//! Each tier is scaled to its own longest edge and is never upscaled. Alpha
//! flattens onto white `(255, 255, 255)`. Every tier file is written as JPEG
//! q90.

use std::borrow::Cow;

use image::codecs::jpeg::JpegEncoder;
use image::imageops::FilterType;
use image::{DynamicImage, Rgb, RgbImage};

use crate::CacheError;

/// The JPEG quality. Fixed.
pub const JPEG_QUALITY: u8 = 90;

/// Scale `source` to fit within `edge` on the longest side.
///
/// A source that already fits is borrowed unchanged, so the cache never
/// upscales and a large source is not copied.
pub(crate) fn fit_to_edge(source: &DynamicImage, edge: u32) -> Cow<'_, DynamicImage> {
    if source.width() <= edge && source.height() <= edge {
        Cow::Borrowed(source)
    } else {
        Cow::Owned(source.resize(edge, edge, FilterType::Lanczos3))
    }
}

/// Flatten alpha onto white.
///
/// An image with no alpha converts straight to RGB, so the large RGBA copy is
/// never built.
pub(crate) fn flatten_onto_white(source: &DynamicImage) -> RgbImage {
    if !source.color().has_alpha() {
        return source.to_rgb8();
    }
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

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use image::{DynamicImage, ImageFormat, Rgb, RgbImage, Rgba, RgbaImage};

    use super::{JPEG_QUALITY, encode_jpeg, fit_to_edge, flatten_onto_white};

    /// The size band in encoded bytes per pixel at quality 90.
    ///
    /// The synthetic image below measures 0.18 to 0.23 bytes per pixel at 256
    /// and 1024 px. The band is wider, so it covers content variation and
    /// still catches a wrong quality or a raw store.
    const SIZE_RANGE: (f64, f64) = (0.10, 0.40);

    /// Build a deterministic RGBA image. The top-left quarter is fully
    /// transparent. The rest is a gradient with hard stripes and a
    /// half-transparent diagonal band.
    fn synthetic_rgba(width: u32, height: u32) -> DynamicImage {
        let mut image = RgbaImage::new(width, height);
        for (x, y, pixel) in image.enumerate_pixels_mut() {
            let fx = f64::from(x) / f64::from(width);
            let fy = f64::from(y) / f64::from(height);
            let base = [
                40.0 + 180.0 * fx,
                60.0 + 150.0 * fy,
                120.0 + 100.0 * (1.0 - fx) * (1.0 - fy),
            ];
            let stripe = ((x / 32) + (y / 32)) % 2 == 0;
            let channel = |value: f64| -> u8 {
                let value = if stripe { value } else { 255.0 - value };
                // The values are inside 0..=255 by construction.
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let byte = value as u8;
                byte
            };
            let alpha = if x < width / 4 && y < height / 4 {
                0
            } else if (x + y) % 97 < 12 {
                128
            } else {
                255
            };
            *pixel = Rgba([channel(base[0]), channel(base[1]), channel(base[2]), alpha]);
        }
        DynamicImage::ImageRgba8(image)
    }

    #[test]
    fn the_quality_constant_is_90() {
        assert_eq!(JPEG_QUALITY, 90);
    }

    #[test]
    fn transparent_pixels_flatten_to_white_through_the_real_encoder() {
        let flat = flatten_onto_white(&synthetic_rgba(64, 64));
        let bytes = encode_jpeg(&flat).expect("encode");
        let decoded = image::load_from_memory_with_format(&bytes, ImageFormat::Jpeg)
            .expect("decode")
            .to_rgb8();

        // Pixel (8, 8) is inside the transparent block and clear of the
        // chroma-subsampling edge.
        let pixel = decoded.get_pixel(8, 8).0;
        assert!(
            pixel.iter().all(|&channel| channel >= 250),
            "transparent pixel is not white after decode: {pixel:?}"
        );
    }

    #[test]
    fn q90_sizes_fall_in_the_expected_range() {
        let (low, high) = SIZE_RANGE;
        for edge in [256_u32, 1024] {
            let flat = flatten_onto_white(&synthetic_rgba(edge, edge));
            let bytes = encode_jpeg(&flat).expect("encode");
            let size = f64::from(u32::try_from(bytes.len()).expect("size"));
            let per_pixel = size / f64::from(edge * edge);
            assert!(
                (low..=high).contains(&per_pixel),
                "{edge} px: {per_pixel:.3} bytes/px is outside {low}-{high}"
            );
        }
    }

    #[test]
    fn an_image_without_alpha_converts_without_blending() {
        let rgb = DynamicImage::ImageRgb8(RgbImage::from_pixel(4, 4, Rgb([1, 2, 3])));
        assert_eq!(flatten_onto_white(&rgb).get_pixel(0, 0).0, [1, 2, 3]);
    }

    #[test]
    fn a_source_that_fits_is_borrowed_and_a_large_one_is_scaled() {
        let small = synthetic_rgba(40, 30);
        assert!(matches!(fit_to_edge(&small, 256), Cow::Borrowed(_)));
        let large = synthetic_rgba(300, 100);
        let fitted = fit_to_edge(&large, 256);
        assert_eq!((fitted.width(), fitted.height()), (256, 85));
    }
}
