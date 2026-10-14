//! JPEG quality-90 cache check (study task 4).
//!
//! Q28 (revised) fixes the cache output: JPEG at quality 90 through the
//! `image` crate, with alpha flattened onto white `(255, 255, 255)`.
//!
//! Two properties are checked here, so the quality gate covers them:
//!
//! 1. A fully transparent pixel becomes white after the round trip.
//! 2. The encoded size at the Q23 tier sizes falls in the expected range.
//!
//! The size range and its justification are in `SIZE_RANGE` below. The timing
//! measurement lives in `examples/jpeg_q90_check.rs`.
//!
//! The numeric casts are for a test harness, not for the library.

#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]

use image::codecs::jpeg::JpegEncoder;
use image::{ImageFormat, Rgb, RgbImage, Rgba, RgbaImage};

/// JPEG quality from Q28 (revised). Fixed.
const QUALITY: u8 = 90;

/// Expected encoded-size band in bytes per pixel at quality 90.
///
/// Justification: the researcher points put a natural-photo JPEG near
/// 0.13-0.16 bytes/px at q80-q85, so q90 sits near 0.1-0.3. The synthetic
/// pattern here is smooth; the measured band is 0.18-0.23 bytes/px. The
/// 0.10-0.40 band covers content variation and catches a wrong quality or a
/// raw store. The measured values are 0.23 bytes/px at 256 px and 0.20 at
/// 1024 px.
const SIZE_RANGE: (f64, f64) = (0.10, 0.40);

#[test]
fn transparent_pixels_flatten_to_white() {
    let source = synthetic_rgba(64, 64);
    let encoded = encode_q90(&flatten_onto_white(&source));
    let decoded = decode(&encoded);

    // The top-left 16x16 block is fully transparent. The center pixel (8, 8)
    // is clear of the chroma-subsampling edge. White is 255; the JPEG
    // tolerance allows a small loss.
    let pixel = decoded.get_pixel(8, 8).0;
    assert!(
        pixel.iter().all(|&channel| channel >= 250),
        "transparent pixel is not white after decode: {pixel:?}"
    );
}

#[test]
fn q90_sizes_fall_in_the_expected_range() {
    let (low, high) = SIZE_RANGE;
    for (width, height) in [(256u32, 256u32), (1024, 1024)] {
        let source = synthetic_rgba(width, height);
        let encoded = encode_q90(&flatten_onto_white(&source));
        let pixels = f64::from(width) * f64::from(height);
        let bytes_per_pixel = encoded.len() as f64 / pixels;
        assert!(
            (low..=high).contains(&bytes_per_pixel),
            "{width}x{height}: {bytes_per_pixel:.3} bytes/px is outside {low}-{high} \
             ({} bytes)",
            encoded.len()
        );
    }
}

/// Build a deterministic RGBA image. The top-left quarter is fully
/// transparent. The rest is a smooth gradient with hard stripes and a
/// half-transparent diagonal band.
fn synthetic_rgba(width: u32, height: u32) -> RgbaImage {
    let mut image = RgbaImage::new(width, height);
    let width_f = width as f32;
    let height_f = height as f32;
    for (x, y, pixel) in image.enumerate_pixels_mut() {
        let fx = x as f32 / width_f;
        let fy = y as f32 / height_f;
        let base_r = 40.0 + 180.0 * fx;
        let base_g = 60.0 + 150.0 * fy;
        let base_b = 120.0 + 100.0 * (1.0 - fx) * (1.0 - fy);
        let stripe = ((x / 32) + (y / 32)) % 2 == 0;
        let (red, green, blue) = if stripe {
            (base_r, base_g, base_b)
        } else {
            (255.0 - base_r, 255.0 - base_g, 255.0 - base_b)
        };
        let transparent = x < width / 4 && y < height / 4;
        let diagonal = (x + y) % 97 < 12;
        let alpha = if transparent {
            0
        } else if diagonal {
            128
        } else {
            255
        };
        *pixel = Rgba([red as u8, green as u8, blue as u8, alpha]);
    }
    image
}

/// Flatten RGBA onto white, matching the Python `paste(img, mask=alpha)` call.
fn flatten_onto_white(source: &RgbaImage) -> RgbImage {
    let mut out = RgbImage::new(source.width(), source.height());
    for (x, y, pixel) in source.enumerate_pixels() {
        let alpha = u32::from(pixel[3]);
        let blend = |channel: u8| -> u8 {
            ((u32::from(channel) * alpha + 255 * (255 - alpha) + 127) / 255) as u8
        };
        out.put_pixel(
            x,
            y,
            Rgb([blend(pixel[0]), blend(pixel[1]), blend(pixel[2])]),
        );
    }
    out
}

/// Encode an RGB image as JPEG at the Q28 quality.
fn encode_q90(image: &RgbImage) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut encoder = JpegEncoder::new_with_quality(&mut bytes, QUALITY);
    encoder.encode_image(image).expect("jpeg encode");
    bytes
}

/// Decode JPEG bytes to RGB.
fn decode(bytes: &[u8]) -> RgbImage {
    image::load_from_memory_with_format(bytes, ImageFormat::Jpeg)
        .expect("jpeg decode")
        .to_rgb8()
}
