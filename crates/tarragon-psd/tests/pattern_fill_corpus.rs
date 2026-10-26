//! The `pattern-fill.psd` sample against its own merged image.
//!
//! The file is not in the repository. The `TARRAGON_PSD_CORPUS` environment
//! variable names the directory that holds it. Without the variable or the
//! file the test prints one skip line and passes.

mod common;

use common::{GOLDEN_TOLERANCE, assert_matches_golden};
use tarragon_psd::{LayerContent, ParseOptions, PsdDocument, Viewport};

/// Premultiply the color of every pixel by its alpha, in place.
fn premultiply(rgba: &mut [u8]) {
    for pixel in rgba.as_chunks_mut::<4>().0.iter_mut() {
        let alpha = u32::from(pixel[3]);
        for channel in &mut pixel[..3] {
            *channel = u8::try_from((u32::from(*channel) * alpha + 127) / 255).unwrap_or(u8::MAX);
        }
    }
}

#[test]
fn the_pattern_fill_sample_matches_its_merged_image() {
    let Some(directory) = std::env::var_os("TARRAGON_PSD_CORPUS") else {
        println!("skip: TARRAGON_PSD_CORPUS is not set");
        return;
    };
    let path = std::path::Path::new(&directory).join("pattern-fill.psd");
    if !path.exists() {
        println!("skip: {} does not exist", path.display());
        return;
    }
    let document = PsdDocument::open(&path).expect("the sample opens");
    let file = document.file();
    let (width, height) = (file.header.width, file.header.height);
    assert!(
        file.layers
            .iter()
            .any(|layer| matches!(layer.content, LayerContent::Pattern(_))),
        "the sample has no pattern fill layer"
    );
    for layer in &file.layers {
        if let LayerContent::Pattern(fill) = &layer.content {
            println!("pattern layer {:?}: {fill:?}", layer.name);
        }
    }
    for tile in &file.patterns {
        println!("pattern {:?} {}x{}", tile.name, tile.width, tile.height);
    }
    let viewport = Viewport::full(width, height);
    let options = ParseOptions::none();

    let merged = document
        .read_merged_region(viewport, &options)
        .expect("the merged image decodes");
    let to_byte = |sample: u16| {
        if merged.depth >= 16 {
            u8::try_from((u32::from(sample) + 128) / 257).unwrap_or(u8::MAX)
        } else {
            u8::try_from(sample).unwrap_or(u8::MAX)
        }
    };
    let mut reference = image::RgbaImage::new(width, height);
    for (x, y, pixel) in reference.enumerate_pixels_mut() {
        let channel = |id: i16| to_byte(merged.sample(id, x, y).unwrap_or(0));
        *pixel = image::Rgba([channel(0), channel(1), channel(2), 255]);
    }

    let composited = document
        .composite_region(viewport, &options)
        .expect("the layers composite");
    let mut got = composited.rgba8();
    let opaque = got.as_chunks::<4>().0.iter().all(|pixel| pixel[3] == 255);
    if !opaque {
        premultiply(&mut got);
        let mut raw = reference.into_raw();
        premultiply(&mut raw);
        reference = image::RgbaImage::from_raw(width, height, raw).expect("the size is kept");
    }

    let (mut max, mut sum, mut close, mut count) = (0u8, 0u64, 0u64, 0u64);
    for (a, b) in got
        .as_chunks::<4>()
        .0
        .iter()
        .zip(reference.as_raw().as_chunks::<4>().0)
    {
        for channel in 0..3 {
            let delta = a[channel].abs_diff(b[channel]);
            max = max.max(delta);
            sum += u64::from(delta);
            close += u64::from(delta <= GOLDEN_TOLERANCE);
            count += 1;
        }
    }
    println!(
        "pattern-fill.psd: max {max}, channel error sum {sum} of {count} channels, \
         {close} within tolerance"
    );
    assert_matches_golden(
        &got,
        width,
        height,
        &reference,
        GOLDEN_TOLERANCE,
        "pattern-fill.psd",
    );
}
