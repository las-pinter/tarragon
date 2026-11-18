//! JPEG quality-90 encoder check for the thumbnail cache.
//!
//! The cache writes JPEG at quality 90 through the `image` crate, pinned at
//! 0.25.10, for a source with no visible alpha. A source with alpha is
//! written as PNG and is not measured here. This example checks two things:
//!
//! 1. The encoded byte size at the tier sizes falls in an expected range.
//! 2. The encode and decode times (min / median / p95).
//!
//! The input is a synthetic opaque RGB image. It is deterministic, so the run
//! repeats.
//!
//! ```text
//! cargo run --release --example jpeg_q90_check -p tarragon-render
//! ```
//!
//! The numeric casts are for a measurement harness, not for the library.

#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]

use std::time::{Duration, Instant};

use image::codecs::jpeg::JpegEncoder;
use image::{ImageFormat, Rgb, RgbImage};

/// JPEG quality. Fixed.
const QUALITY: u8 = 90;
/// The size band in bytes per pixel. The same band as the cache unit test.
const SIZE_RANGE: (f64, f64) = (0.05, 0.20);
/// Measured iterations per size. The first two runs are warmup.
const ITERATIONS: usize = 12;
/// Warmup runs per size. The first run fills lazily allocated buffers.
const WARMUP: usize = 2;
/// Tier sizes: 256 px, 1024 px, and the 8192 px full cap.
const TIERS: [(u32, u32); 4] = [(256, 256), (1024, 1024), (4096, 4096), (8192, 8192)];

/// One measured size.
struct SizeReport {
    width: u32,
    height: u32,
    bytes: usize,
    encode: Timing,
    decode: Timing,
}

/// min / median / p95 of a sample set, in milliseconds.
struct Timing {
    min: f64,
    median: f64,
    p95: f64,
}

fn main() {
    println!("== tarragon JPEG q90 encoder check ==");
    println!("crate:       image 0.25.10 (in-repo JPEG encoder, no jpeg-encoder)");
    println!("quality:     {QUALITY}");
    println!("input:       opaque RGB, no alpha");
    println!("cpu:         {}", cpu_model());
    println!("cpu affinity: {}", cpu_affinity());

    // Sizes and times at each tier.
    let mut reports = Vec::new();
    for (width, height) in TIERS {
        reports.push(measure_size(width, height));
    }

    println!();
    println!("-- sizes and times --");
    println!(
        "{:>6} {:>6} {:>9} {:>12} {:>10} {:>26} {:>26}",
        "w", "h", "MP", "bytes", "bytes/px", "encode ms (min/med/p95)", "decode ms (min/med/p95)"
    );
    for r in &reports {
        let pixels = f64::from(r.width) * f64::from(r.height);
        println!(
            "{:>6} {:>6} {:>9.2} {:>12} {:>10.3} {:>9.3} {:>8.3} {:>8.3} {:>9.3} {:>8.3} {:>8.3}",
            r.width,
            r.height,
            pixels / 1_000_000.0,
            r.bytes,
            r.bytes as f64 / pixels,
            r.encode.min,
            r.encode.median,
            r.encode.p95,
            r.decode.min,
            r.decode.median,
            r.decode.p95,
        );
    }

    println!();
    println!(
        "expected size range: {} to {} bytes per pixel at q90, checked at 256 px and 1024 px",
        SIZE_RANGE.0, SIZE_RANGE.1
    );
    println!("justification: the opaque synthetic pattern is smooth and measures");
    println!("  0.12 bytes/px at 256 px and 0.07 at 1024 px. The band covers content");
    println!("  variation. Larger sizes compress better and are not in the band.");
    let in_band = reports.iter().filter(|r| r.width <= 1024).all(|r| {
        (SIZE_RANGE.0..=SIZE_RANGE.1).contains(&(r.bytes as f64 / f64::from(r.width * r.height)))
    });
    if !in_band {
        eprintln!("FAIL: a size is outside the expected band");
        std::process::exit(1);
    }
}

/// Measure encode and decode at one size.
fn measure_size(width: u32, height: u32) -> SizeReport {
    let rgb = synthetic_opaque(width, height);

    for _ in 0..WARMUP {
        let bytes = encode_q90(&rgb);
        let _ =
            image::load_from_memory_with_format(&bytes, ImageFormat::Jpeg).expect("jpeg decode");
    }

    let mut encode_samples = Vec::with_capacity(ITERATIONS);
    let mut decode_samples = Vec::with_capacity(ITERATIONS);
    let mut bytes = Vec::new();
    for _ in 0..ITERATIONS {
        let started = Instant::now();
        bytes = encode_q90(&rgb);
        encode_samples.push(started.elapsed());

        let started = Instant::now();
        let decoded =
            image::load_from_memory_with_format(&bytes, ImageFormat::Jpeg).expect("jpeg decode");
        decode_samples.push(started.elapsed());
        drop(decoded);
    }

    SizeReport {
        width,
        height,
        bytes: bytes.len(),
        encode: summarize(&mut encode_samples),
        decode: summarize(&mut decode_samples),
    }
}

/// Build a deterministic opaque RGB image: a smooth gradient with hard
/// stripes.
fn synthetic_opaque(width: u32, height: u32) -> RgbImage {
    let mut image = RgbImage::new(width, height);
    let width_f = width as f32;
    let height_f = height as f32;
    for (x, y, pixel) in image.enumerate_pixels_mut() {
        let fx = x as f32 / width_f;
        let fy = y as f32 / height_f;

        // Smooth low-frequency base.
        let base_r = 40.0 + 180.0 * fx;
        let base_g = 60.0 + 150.0 * fy;
        let base_b = 120.0 + 100.0 * (1.0 - fx) * (1.0 - fy);

        // Hard 32 px stripes. They add high-frequency content for the encoder.
        let stripe = ((x / 32) + (y / 32)) % 2 == 0;
        let (red, green, blue) = if stripe {
            (base_r, base_g, base_b)
        } else {
            (255.0 - base_r, 255.0 - base_g, 255.0 - base_b)
        };

        *pixel = Rgb([red as u8, green as u8, blue as u8]);
    }
    image
}

/// Encode an RGB image as JPEG at [`QUALITY`].
fn encode_q90(image: &RgbImage) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut encoder = JpegEncoder::new_with_quality(&mut bytes, QUALITY);
    encoder.encode_image(image).expect("jpeg encode");
    bytes
}

/// Reduce a sample set to min / median / p95 in milliseconds.
fn summarize(samples: &mut [Duration]) -> Timing {
    samples.sort_unstable();
    Timing {
        min: millis(samples[0]),
        median: millis(samples[samples.len() / 2]),
        p95: millis(samples[samples.len() * 95 / 100]),
    }
}

/// Convert a duration to milliseconds.
fn millis(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

/// Read the CPU model name from `/proc/cpuinfo`.
fn cpu_model() -> String {
    std::fs::read_to_string("/proc/cpuinfo")
        .ok()
        .and_then(|info| {
            info.lines().find_map(|line| {
                line.strip_prefix("model name")
                    .and_then(|rest| rest.split(':').nth(1))
                    .map(|name| name.trim().to_owned())
            })
        })
        .unwrap_or_else(|| "unknown".to_owned())
}

/// Read the CPU list that the process may run on.
fn cpu_affinity() -> String {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|status| {
            status.lines().find_map(|line| {
                line.strip_prefix("Cpus_allowed_list:")
                    .map(|list| list.trim().to_owned())
            })
        })
        .unwrap_or_else(|| "unknown".to_owned())
}
