//! The `adjustment-layers` sample: sixteen adjustment layers over one pixel layer.
//!
//! The files are not in the repository. The `TARRAGON_PSD_CORPUS` environment
//! variable names the directory that holds `adjustment-layers.psd` and its
//! reference `adjustment-layers.png`. Without the variable or the PSD file each
//! test prints one skip line and passes.
//!
//! The ignored diagnostic needs the files too. Run it with `--run-ignored only`.

mod common;

use common::{GOLDEN_TOLERANCE, assert_matches_golden};
use tarragon_psd::{LayerContent, ParseOptions, PsdDocument, Viewport};

/// Premultiply the color of every pixel by its alpha, in place.
///
/// The reference PNG was stored from 8-bit premultiplied data, so its color
/// carries no information where alpha is small. Both sides are compared
/// premultiplied.
fn premultiply(rgba: &mut [u8]) {
    for pixel in rgba.as_chunks_mut::<4>().0.iter_mut() {
        let alpha = u32::from(pixel[3]);
        for channel in &mut pixel[..3] {
            *channel = u8::try_from((u32::from(*channel) * alpha + 127) / 255).unwrap_or(u8::MAX);
        }
    }
}

/// The sample bytes and its reference image, or `None` after a skip line.
fn load() -> Option<(Vec<u8>, image::RgbaImage)> {
    let Some(directory) = std::env::var_os("TARRAGON_PSD_CORPUS") else {
        println!("skip: TARRAGON_PSD_CORPUS is not set");
        return None;
    };
    let path = std::path::Path::new(&directory).join("adjustment-layers.psd");
    if !path.exists() {
        println!("skip: {} does not exist", path.display());
        return None;
    }
    let bytes = std::fs::read(&path).expect("the sample reads");
    let png = std::path::Path::new(&directory).join("adjustment-layers.png");
    let reference = image::open(&png)
        .unwrap_or_else(|e| panic!("{}: {e}", png.display()))
        .to_rgba8();
    let (width, height) = reference.dimensions();
    let mut raw = reference.into_raw();
    premultiply(&mut raw);
    let reference = image::RgbaImage::from_raw(width, height, raw).expect("the size is kept");
    Some((bytes, reference))
}

/// Composite the whole canvas to premultiplied RGBA8.
fn composite(bytes: Vec<u8>) -> (Vec<u8>, u32, u32) {
    let document = PsdDocument::from_bytes(bytes, &ParseOptions::none()).expect("the sample opens");
    let header = &document.file().header;
    let (width, height) = (header.width, header.height);
    let region = document
        .composite_region(Viewport::full(width, height), &ParseOptions::none())
        .expect("the layers composite");
    let mut rgba = region.rgba8();
    premultiply(&mut rgba);
    (rgba, width, height)
}

#[test]
fn every_adjustment_layer_in_the_sample_is_supported() {
    let Some((bytes, _)) = load() else { return };
    let document = PsdDocument::from_bytes(bytes, &ParseOptions::none()).expect("the sample opens");
    let mut adjustments = 0;
    for layer in &document.file().layers {
        let state = match &layer.content {
            LayerContent::Adjustment(_) => {
                adjustments += 1;
                "adjustment"
            }
            LayerContent::Unsupported => "UNSUPPORTED",
            _ => "other",
        };
        println!("{:<28} {state}", layer.name);
        assert!(
            layer.content != LayerContent::Unsupported,
            "{} is unsupported",
            layer.name
        );
    }
    assert!(adjustments >= 16, "only {adjustments} adjustment layers");
}

/// The full composite against the reference image, at the golden tolerance.
///
/// Both images are compared premultiplied, because the reference PNG was
/// stored from 8-bit premultiplied data and its color is meaningless where
/// alpha is 1 to 3. The sample has empty masks on all adjustment layers, and
/// the chain ends in Channel Mixer, Invert, Posterize and Threshold, which
/// leave two values. So it checks the end of the stack, not each earlier
/// adjustment.
#[test]
fn the_adjustment_sample_matches_its_reference_image() {
    let Some((bytes, reference)) = load() else {
        return;
    };
    let (rgba, width, height) = composite(bytes);
    assert_eq!(alpha_misses(&rgba, reference.as_raw()), 0, "alpha differs");
    assert_matches_golden(
        &rgba,
        width,
        height,
        &reference,
        GOLDEN_TOLERANCE,
        "adjustment-layers.psd",
    );
}

/// The number of pixels whose alpha differs by more than the tolerance.
fn alpha_misses(got: &[u8], want: &[u8]) -> usize {
    got.as_chunks::<4>()
        .0
        .iter()
        .zip(want.as_chunks::<4>().0.iter())
        .filter(|(a, b)| a[3].abs_diff(b[3]) > GOLDEN_TOLERANCE)
        .count()
}

/// The max and mean RGB difference, and the share of channels within 2 levels.
fn compare(got: &[u8], want: &[u8]) -> (u8, f64, f64) {
    let mut max = 0u8;
    let mut sum = 0u64;
    let mut close = 0u64;
    let mut count = 0u64;
    for (a, b) in got
        .as_chunks::<4>()
        .0
        .iter()
        .zip(want.as_chunks::<4>().0.iter())
    {
        for k in 0..3 {
            let d = a[k].abs_diff(b[k]);
            max = max.max(d);
            sum += u64::from(d);
            close += u64::from(d <= GOLDEN_TOLERANCE);
            count += 1;
        }
    }
    #[allow(clippy::cast_precision_loss, reason = "a report, not a result")]
    let (mean, share) = (sum as f64 / count as f64, close as f64 / count as f64);
    (max, mean, share)
}

/// The byte offsets of the layer flag bytes, in record order.
fn flag_offsets(data: &[u8]) -> Vec<usize> {
    let be32 = |at: usize| u32::from_be_bytes(data[at..at + 4].try_into().expect("4 bytes"));
    let mut at = 26;
    at += 4 + be32(at) as usize; // color mode data
    at += 4 + be32(at) as usize; // image resources
    at += 8; // layer and mask length, layer info length
    let count = i16::from_be_bytes([data[at], data[at + 1]]).unsigned_abs();
    at += 2;
    let mut offsets = Vec::new();
    for _ in 0..count {
        let channels = usize::from(u16::from_be_bytes([data[at + 16], data[at + 17]]));
        let after = at + 18 + channels * 6 + 8; // blend signature and key
        offsets.push(after + 2);
        at = after + 4 + 4 + be32(after + 4) as usize; // flags, filler, extra length
    }
    offsets
}

/// Report how far the stack is from the reference, and which layer matters.
///
/// Run: `TARRAGON_PSD_CORPUS=<dir> cargo nextest run -p tarragon-psd
/// diagnose_the_stack --run-ignored only --no-capture`.
///
/// Every adjustment layer in the sample has an empty layer mask with default
/// 255, so each one covers the whole canvas. The Channel Mixer turns the
/// image to grey, and Invert, Posterize and Threshold then reduce it to two
/// values. Hiding one of the layers before the Channel Mixer changes nothing,
/// so the sample cannot check those layers one by one.
#[test]
#[ignore = "diagnostic: prints the differences of the sample, needs TARRAGON_PSD_CORPUS"]
fn diagnose_the_stack() {
    let Some((bytes, reference)) = load() else {
        return;
    };
    let want = reference.as_raw().clone();
    let (full, _, _) = composite(bytes.clone());
    let (max, mean, share) = compare(&full, &want);
    println!("alpha misses: {}", alpha_misses(&full, &want));
    println!(
        "full composite: max {max}, mean {mean:.3}, within 2: {:.2}%",
        share * 100.0
    );

    let width = 200usize;
    let mut shown = 0;
    let mut columns = [usize::MAX, 0usize];
    let mut rows = [usize::MAX, 0usize];
    for (index, (a, b)) in full
        .as_chunks::<4>()
        .0
        .iter()
        .zip(want.as_chunks::<4>().0.iter())
        .enumerate()
    {
        if (0..3).any(|k| a[k].abs_diff(b[k]) > GOLDEN_TOLERANCE) {
            let (x, y) = (index % width, index / width);
            columns = [columns[0].min(x), columns[1].max(x)];
            rows = [rows[0].min(y), rows[1].max(y)];
            if shown < 8 {
                println!("over at ({x}, {y}): got {:?} want {:?}", &a[..3], &b[..3]);
                shown += 1;
            }
        }
    }
    println!("pixels over 2 span columns {columns:?} rows {rows:?}");
    let document = PsdDocument::from_bytes(bytes.clone(), &ParseOptions::none()).expect("opens");
    let names: Vec<String> = document
        .file()
        .layers
        .iter()
        .map(|l| l.name.clone())
        .collect();
    for layer in &document.file().layers {
        if let Some(mask) = layer.mask {
            let b = mask.bounds;
            println!(
                "mask of {:<24} rect ({}, {})-({}, {}) default {}",
                layer.name, b.left, b.top, b.right, b.bottom, mask.default_color
            );
        }
    }
    for (index, offset) in flag_offsets(&bytes).into_iter().enumerate() {
        let mut copy = bytes.clone();
        copy[offset] |= 2;
        let (hidden, _, _) = composite(copy);
        let (change_max, change_mean, _) = compare(&hidden, &full);
        let (err_max, err_mean, err_share) = compare(&hidden, &want);
        println!(
            "hide {:<24} change max {change_max:>3} mean {change_mean:>7.3} | error max {err_max:>3} mean {err_mean:>7.3} within2 {:.2}%",
            names.get(index).map_or("?", String::as_str),
            err_share * 100.0
        );
    }
}
