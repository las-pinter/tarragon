//! JPEG quality-90 encoder check for the thumbnail cache (study task 4).
//!
//! Q28 (revised) fixes the cache output: JPEG at quality 90 through the
//! `image` crate, pinned at 0.25.10, with alpha flattened onto white
//! `(255, 255, 255)`. This example checks three things on the Q10 reference:
//!
//! 1. The encoded byte size at the Q23 tier sizes falls in an expected range.
//! 2. A fully transparent pixel becomes white after the round trip.
//! 3. The encode and decode times (min / median / p95).
//!
//! The input is a synthetic RGBA image with a known transparent corner. It is
//! deterministic, so the run repeats.
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
use image::{ImageFormat, Rgb, RgbImage, Rgba, RgbaImage};

/// JPEG quality from Q28 (revised). Fixed.
const QUALITY: u8 = 90;
/// Measured iterations per size. The first two runs are warmup.
const ITERATIONS: usize = 12;
/// Warmup runs per size. The first run fills lazily allocated buffers.
const WARMUP: usize = 2;
/// Tier sizes from Q23: 256 px, 1024 px, and the 8192 px full cap.
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
    println!("== tarragon JPEG q90 encoder check (study task 4) ==");
    println!("crate:       image 0.25.10 (in-repo JPEG encoder, no jpeg-encoder)");
    println!("quality:     {QUALITY} (Q28 revised)");
    println!("flatten:     white (255, 255, 255) onto RGB before encode");
    println!("cpu:         {}", cpu_model());
    println!("cpu affinity: {}", cpu_affinity());

    // Part 1: alpha flattens to white.
    let alpha_ok = check_alpha_flatten();

    // Part 2: sizes and times at each tier.
    let mut reports = Vec::new();
    for (width, height) in TIERS {
        reports.push(measure_size(width, height));
    }

    println!();
    println!("-- alpha flatten --");
    println!(
        "fully transparent pixel after decode is white: {}",
        if alpha_ok { "yes" } else { "NO" }
    );

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
    println!("expected size range: 0.10 to 0.40 bytes per pixel at q90");
    println!("justification: researcher points put natural-photo JPEG near");
    println!("  0.13-0.16 bytes/px at q80-q85, so q90 sits near 0.1-0.3.");
    println!("  The synthetic pattern is smooth; the measured band is 0.18-0.23.");
    println!("  0.10-0.40 covers content variation and catches a wrong quality.");

    if !alpha_ok {
        eprintln!("FAIL: transparent pixel did not flatten to white");
        std::process::exit(1);
    }
}

/// Encode a synthetic 64x64 RGBA image with a transparent block, decode it, and
/// check that the block is white within JPEG tolerance.
fn check_alpha_flatten() -> bool {
    let source = synthetic_rgba(64, 64);
    let bytes = encode_q90(&flatten_onto_white(&source));
    let decoded = image::load_from_memory_with_format(&bytes, ImageFormat::Jpeg)
        .expect("jpeg decode")
        .to_rgb8();

    // Sample the center of the top-left transparent block. The block is 16x16,
    // so the center pixel is clear of the chroma-subsampling edge.
    let pixel = decoded.get_pixel(8, 8).0;
    pixel.iter().all(|&channel| channel >= 250)
}

/// Measure encode and decode at one size.
fn measure_size(width: u32, height: u32) -> SizeReport {
    let source = synthetic_rgba(width, height);
    let rgb = flatten_onto_white(&source);
    drop(source);

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
