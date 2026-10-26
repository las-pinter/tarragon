//! Image scaling and encoding for the tier files.
//!
//! Each tier is scaled to its own longest edge and is never upscaled. A
//! source with visible alpha is written as PNG so the alpha survives. Any
//! other source is written as JPEG q90.

use std::borrow::Cow;
use std::io::Cursor;

use image::codecs::jpeg::JpegEncoder;
use image::imageops::FilterType;
use image::{DynamicImage, ImageFormat, RgbImage};
use tarragon_core::tier::Tier;

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

/// Return `source` scaled for the 256 px tier.
///
/// The result is the image the thumb tier encodes. A small source is not
/// upscaled.
#[must_use]
pub fn thumb_image(source: &DynamicImage) -> DynamicImage {
    let edge = Tier::Thumb256.max_edge().unwrap_or(256);
    fit_to_edge(source, edge).into_owned()
}

/// The file format of the tier files of one source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TierFormat {
    /// JPEG q90, for a source with no visible alpha.
    Jpeg,
    /// PNG, for a source with at least one pixel that is not fully opaque.
    Png,
}

impl TierFormat {
    /// Both formats, in the order a lookup probes them.
    pub const PROBE_ORDER: [Self; 2] = [Self::Png, Self::Jpeg];

    /// Return the file extension without the dot.
    #[must_use]
    pub fn extension(self) -> &'static str {
        match self {
            Self::Jpeg => "jpg",
            Self::Png => "png",
        }
    }

    /// Return the other format.
    #[must_use]
    pub fn other(self) -> Self {
        match self {
            Self::Jpeg => Self::Png,
            Self::Png => Self::Jpeg,
        }
    }

    /// Pick the format for `source`: PNG when any pixel is not fully opaque,
    /// else JPEG.
    #[must_use]
    pub fn for_source(source: &DynamicImage) -> Self {
        if has_visible_alpha(source) {
            Self::Png
        } else {
            Self::Jpeg
        }
    }
}

/// Return true when `source` has an alpha channel and at least one pixel with
/// alpha below the maximum.
///
/// An opaque RGBA image returns false, so it stays JPEG. The scan borrows the
/// pixels and builds no copy.
#[must_use]
pub(crate) fn has_visible_alpha(source: &DynamicImage) -> bool {
    match source {
        DynamicImage::ImageLumaA8(img) => img.pixels().any(|p| p.0[1] < u8::MAX),
        DynamicImage::ImageRgba8(img) => img.pixels().any(|p| p.0[3] < u8::MAX),
        DynamicImage::ImageLumaA16(img) => img.pixels().any(|p| p.0[1] < u16::MAX),
        DynamicImage::ImageRgba16(img) => img.pixels().any(|p| p.0[3] < u16::MAX),
        DynamicImage::ImageRgba32F(img) => img.pixels().any(|p| p.0[3] < 1.0),
        // An alpha layout added later still gets a correct answer, at the
        // price of one 8-bit copy.
        other if other.color().has_alpha() => other.to_rgba8().pixels().any(|p| p.0[3] < u8::MAX),
        _ => false,
    }
}

/// Encode `image` as a tier payload of the given format.
///
/// # Errors
///
/// Returns [`CacheError::Encode`] when the encoder fails.
pub(crate) fn encode_tier(image: &DynamicImage, format: TierFormat) -> Result<Vec<u8>, CacheError> {
    match format {
        TierFormat::Jpeg => encode_jpeg(&image.to_rgb8()),
        TierFormat::Png => encode_png(image),
    }
}

/// Encode an image as 8-bit RGBA PNG, which keeps alpha.
///
/// A 16-bit or `f32` source is reduced to 8 bits per channel, which is enough
/// for a cache tier.
///
/// # Errors
///
/// Returns [`CacheError::Encode`] when the encoder fails.
pub(crate) fn encode_png(image: &DynamicImage) -> Result<Vec<u8>, CacheError> {
    let mut bytes = Vec::new();
    image
        .to_rgba8()
        .write_to(&mut Cursor::new(&mut bytes), ImageFormat::Png)
        .map_err(CacheError::Encode)?;
    Ok(bytes)
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

    use image::codecs::jpeg::JpegEncoder;
    use image::{
        DynamicImage, GrayAlphaImage, GrayImage, ImageBuffer, ImageFormat, LumaA, RgbImage, Rgba,
        RgbaImage,
    };

    use crate::store::CacheStore;

    use super::{
        JPEG_QUALITY, TierFormat, encode_jpeg, encode_png, fit_to_edge, has_visible_alpha,
        thumb_image,
    };

    type Rgba16Image = ImageBuffer<Rgba<u16>, Vec<u16>>;

    /// The size band in encoded bytes per pixel at quality 90.
    ///
    /// The opaque synthetic image below measures 0.12 bytes per pixel at 256
    /// px and 0.07 at 1024 px. The band is wider, so it covers content
    /// variation. A wrong quality is caught by the q75 comparison below. The
    /// older, higher figure came from the half-alpha band blended with white,
    /// which this opaque image no longer has.
    const SIZE_RANGE: (f64, f64) = (0.05, 0.20);

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

    /// The synthetic image with every pixel made opaque.
    fn synthetic_opaque(width: u32, height: u32) -> RgbImage {
        let mut image = synthetic_rgba(width, height).to_rgba8();
        for pixel in image.pixels_mut() {
            pixel.0[3] = u8::MAX;
        }
        DynamicImage::ImageRgba8(image).to_rgb8()
    }

    #[test]
    fn the_quality_constant_is_90() {
        assert_eq!(JPEG_QUALITY, 90);
    }

    #[test]
    fn png_keeps_alpha_through_the_real_encoder() {
        let bytes = encode_png(&synthetic_rgba(64, 64)).expect("encode");
        assert_eq!(bytes.get(..4), Some([0x89, b'P', b'N', b'G'].as_slice()));
        let decoded = image::load_from_memory_with_format(&bytes, ImageFormat::Png)
            .expect("decode")
            .to_rgba8();
        assert_eq!(decoded.get_pixel(8, 8).0[3], 0);
    }

    #[test]
    fn alpha_detection_needs_a_pixel_below_the_maximum() {
        let opaque = RgbaImage::from_pixel(4, 4, Rgba([1, 2, 3, 255]));
        assert!(!has_visible_alpha(&DynamicImage::ImageRgba8(
            opaque.clone()
        )));
        let mut one = opaque;
        one.put_pixel(2, 3, Rgba([1, 2, 3, 254]));
        assert!(has_visible_alpha(&DynamicImage::ImageRgba8(one)));
        assert!(has_visible_alpha(&synthetic_rgba(8, 8)));
    }

    #[test]
    fn alpha_detection_covers_the_other_layouts() {
        let rgb = DynamicImage::ImageRgb8(RgbImage::new(2, 2));
        assert!(!has_visible_alpha(&rgb));
        let luma = DynamicImage::ImageLuma8(GrayImage::new(2, 2));
        assert!(!has_visible_alpha(&luma));
        let la_opaque =
            DynamicImage::ImageLumaA8(GrayAlphaImage::from_pixel(2, 2, LumaA([9, 255])));
        assert!(!has_visible_alpha(&la_opaque));
        let la = DynamicImage::ImageLumaA8(GrayAlphaImage::from_pixel(2, 2, LumaA([9, 0])));
        assert!(has_visible_alpha(&la));
        let wide_opaque = Rgba16Image::from_pixel(2, 2, Rgba([1, 2, 3, u16::MAX]));
        assert!(!has_visible_alpha(&DynamicImage::ImageRgba16(wide_opaque)));
        let mut wide = Rgba16Image::from_pixel(2, 2, Rgba([1, 2, 3, u16::MAX]));
        wide.put_pixel(1, 1, Rgba([1, 2, 3, u16::MAX - 1]));
        assert!(has_visible_alpha(&DynamicImage::ImageRgba16(wide)));
        let la16: ImageBuffer<LumaA<u16>, Vec<u16>> =
            ImageBuffer::from_pixel(2, 2, LumaA([7, u16::MAX]));
        assert!(!has_visible_alpha(&DynamicImage::ImageLumaA16(la16)));
        let la16: ImageBuffer<LumaA<u16>, Vec<u16>> =
            ImageBuffer::from_pixel(2, 2, LumaA([7, 100]));
        assert!(has_visible_alpha(&DynamicImage::ImageLumaA16(la16)));
        let float: ImageBuffer<Rgba<f32>, Vec<f32>> =
            ImageBuffer::from_pixel(2, 2, Rgba([0.1, 0.2, 0.3, 1.0]));
        assert!(!has_visible_alpha(&DynamicImage::ImageRgba32F(
            float.clone()
        )));
        let mut float = float;
        float.put_pixel(0, 0, Rgba([0.1, 0.2, 0.3, 0.5]));
        assert!(has_visible_alpha(&DynamicImage::ImageRgba32F(float)));
    }

    #[test]
    fn the_format_extension_and_other_are_consistent() {
        assert_eq!(TierFormat::Jpeg.extension(), "jpg");
        assert_eq!(TierFormat::Png.extension(), "png");
        assert_eq!(TierFormat::Jpeg.other(), TierFormat::Png);
        assert_eq!(TierFormat::Png.other(), TierFormat::Jpeg);
    }

    #[test]
    fn q90_sizes_fall_in_the_expected_range() {
        let (low, high) = SIZE_RANGE;
        for edge in [256_u32, 1024] {
            let flat = synthetic_opaque(edge, edge);
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
    fn q90_is_larger_than_q75_for_the_same_image() {
        let flat = synthetic_opaque(256, 256);
        let q90 = encode_jpeg(&flat).expect("encode").len();
        let mut q75 = Vec::new();
        JpegEncoder::new_with_quality(&mut q75, 75)
            .encode_image(&flat)
            .expect("encode q75");
        assert!(
            q90 > q75.len(),
            "q90 {q90} bytes is not above q75 {}",
            q75.len()
        );
    }

    #[test]
    fn a_source_that_fits_is_borrowed_and_a_large_one_is_scaled() {
        let small = synthetic_rgba(40, 30);
        assert!(matches!(fit_to_edge(&small, 256), Cow::Borrowed(_)));
        let large = synthetic_rgba(300, 100);
        let fitted = fit_to_edge(&large, 256);
        assert_eq!((fitted.width(), fitted.height()), (256, 85));
    }

    #[test]
    fn the_thumb_helper_matches_the_256_tier_file() {
        let large = synthetic_rgba(600, 300);
        let dir = std::env::temp_dir().join(format!("tarragon-thumb-{}", std::process::id()));
        let store = CacheStore::new(&dir);
        let entry = store
            .write_all(&large, "gallery", "abcd1234", "art.png")
            .expect("write the tiers");
        assert_eq!(
            entry.format,
            TierFormat::Png,
            "alpha keeps the tier lossless"
        );
        let decoded = image::open(&entry.thumb256).expect("decode the 256 tier");
        let _ = std::fs::remove_dir_all(&dir);

        let thumb = thumb_image(&large);
        assert_eq!((thumb.width(), thumb.height()), (256, 128));
        assert_eq!(decoded.to_rgba8(), thumb.to_rgba8());

        let tall = thumb_image(&synthetic_rgba(100, 900));
        assert_eq!(tall.height(), 256);
        assert!(tall.width() <= 256);
        let small = synthetic_rgba(40, 30);
        assert_eq!(thumb_image(&small), small);
    }
}
