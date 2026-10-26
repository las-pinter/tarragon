//! The `vector_mask.psd` sample against its own merged image.
//!
//! The file is not in the repository. The `TARRAGON_PSD_CORPUS` environment
//! variable names the directory that holds it. Without the variable or the
//! file the test prints one skip line and passes.

mod common;

use common::{GOLDEN_TOLERANCE, assert_matches_golden};
use tarragon_psd::{ParseOptions, PsdDocument, Viewport};

#[test]
fn the_vector_mask_sample_matches_its_merged_image() {
    let Some(directory) = std::env::var_os("TARRAGON_PSD_CORPUS") else {
        println!("skip: TARRAGON_PSD_CORPUS is not set");
        return;
    };
    let path = std::path::Path::new(&directory).join("vector_mask.psd");
    if !path.exists() {
        println!("skip: {} does not exist", path.display());
        return;
    }
    let document = PsdDocument::open(&path).expect("the sample opens");
    let header = &document.file().header;
    let (width, height) = (header.width, header.height);
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
    assert_matches_golden(
        &composited.rgba8(),
        width,
        height,
        &reference,
        GOLDEN_TOLERANCE,
        "vector_mask.psd",
    );
}
