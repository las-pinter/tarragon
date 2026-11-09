//! The `vector_mask.psd` sample against its own merged image.
//!
//! The reference is the production merged path, `read_merged_pixels`. This
//! file stores its merged color matted over white, so the raw samples differ
//! from the straight color of the composite where alpha is below one. Both
//! sides are compared premultiplied, where the two agree. The golden compare
//! ignores alpha, so alpha is checked in a separate loop. Both sides share the
//! sample decoding (`rgb_color`, `channel_samples`), so this test does not
//! check decoding.
//!
//! The merged alpha comes from the raster mask alone. The sample shows that the
//! vector coverage does not clip where the file has content. It does not show
//! that the vector shape is right.
//!
//! The file is not in the repository. The `TARRAGON_PSD_CORPUS` environment
//! variable names the directory that holds it. Without the variable or the
//! file the test prints one skip line and passes.

mod common;

use common::{GOLDEN_TOLERANCE, assert_matches_golden, premultiply};
use tarragon_psd::{ParseOptions, PsdDocument, Viewport};

/// The composite of the sample matches its merged image in alpha and in
/// premultiplied color.
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

    let reference = document
        .read_merged_pixels(viewport, 1, &options)
        .expect("the merged image decodes")
        .rgba8();
    let composited = document
        .composite_region(viewport, &options)
        .expect("the layers composite")
        .rgba8();

    let columns = usize::try_from(width).expect("width");
    for (index, (got, want)) in composited
        .as_chunks::<4>()
        .0
        .iter()
        .zip(reference.as_chunks::<4>().0.iter())
        .enumerate()
    {
        assert!(
            got[3].abs_diff(want[3]) <= GOLDEN_TOLERANCE,
            "vector_mask.psd: alpha {} against merged alpha {} at ({}, {})",
            got[3],
            want[3],
            index % columns,
            index / columns
        );
    }

    let (mut got, mut want) = (composited, reference);
    premultiply(&mut got);
    premultiply(&mut want);
    let want = image::RgbaImage::from_raw(width, height, want).expect("the reference size");
    assert_matches_golden(
        &got,
        width,
        height,
        &want,
        GOLDEN_TOLERANCE,
        "vector_mask.psd",
    );
}
