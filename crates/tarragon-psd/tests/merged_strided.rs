//! Stepped reads of the merged image, and the lenient open.
//!
//! The stepped read keeps every Nth pixel and skips the rows between. The
//! tests compare it with the plain read, so the step cannot drift from the
//! full-resolution pixels.

mod common;

use common::{DocumentSpec, LayerSpec, build_psd, push_u16, rle_merged_section};
use tarragon_psd::{ParseOptions, PsdDocument, PsdError, Viewport};

/// One plane of a pattern that differs at every pixel.
fn pattern(width: usize, height: usize, seed: usize) -> Vec<u8> {
    (0..width * height)
        .map(|index| u8::try_from((index * 7 + index / width * 13 + seed * 50) % 251).expect("u8"))
        .collect()
}

/// An RGB document whose merged planes are the three patterns.
fn document(width: usize, height: usize, rle: bool) -> PsdDocument {
    let planes: Vec<Vec<u8>> = (0..3).map(|seed| pattern(width, height, seed)).collect();
    let mut spec = DocumentSpec::new(
        u32::try_from(width).expect("width"),
        u32::try_from(height).expect("height"),
        8,
        3,
    );
    if rle {
        spec.merged_section = Some(rle_merged_section(width, &planes, false));
    } else {
        spec.merged = planes
            .iter()
            .map(|plane| plane.iter().map(|value| u16::from(*value)).collect())
            .collect();
    }
    PsdDocument::from_bytes(build_psd(&spec), &ParseOptions::none()).expect("parses")
}

/// Assert that a stepped read equals the plain read, sampled.
fn assert_step_matches(document: &PsdDocument, view: Viewport, step: u32) {
    let options = ParseOptions::none();
    let full = document.read_merged_region(view, &options).expect("full");
    let kept = document
        .read_merged_strided(view, step, &options)
        .expect("stepped");
    assert_eq!(kept.viewport.width(), view.width().div_ceil(step));
    assert_eq!(kept.viewport.height(), view.height().div_ceil(step));
    for id in 0..3 {
        for y in 0..kept.viewport.height() {
            for x in 0..kept.viewport.width() {
                assert_eq!(
                    kept.sample(id, x, y),
                    full.sample(id, x * step, y * step),
                    "channel {id} at {x},{y} step {step}"
                );
            }
        }
    }
}

#[test]
fn a_stepped_read_keeps_every_nth_pixel_of_a_raw_image() {
    let document = document(37, 23, false);
    for step in [1, 2, 3, 5, 40] {
        assert_step_matches(&document, Viewport::full(37, 23), step);
    }
}

#[test]
fn a_stepped_read_keeps_every_nth_pixel_of_an_rle_image() {
    let document = document(37, 23, true);
    for step in [1, 2, 3, 5, 40] {
        assert_step_matches(&document, Viewport::full(37, 23), step);
    }
}

#[test]
fn a_stepped_read_of_a_band_starts_its_grid_at_the_band_origin() {
    let document = document(37, 23, true);
    assert_step_matches(&document, Viewport::new(0, 6, 37, 18), 3);
    assert_step_matches(&document, Viewport::new(5, 4, 35, 22), 4);
}

#[test]
fn the_cap_applies_to_the_output_of_a_stepped_read() {
    // A gray canvas wider than the cap, one flat row repeated.
    let (width, height) = (8200usize, 4usize);
    let mut spec = DocumentSpec::new(8200, 4, 8, 1);
    spec.merged_section = Some(rle_merged_section(
        width,
        &[vec![77u8; width * height]],
        false,
    ));
    let document = PsdDocument::from_bytes(build_psd(&spec), &ParseOptions::none()).expect("ok");
    let options = ParseOptions::none();
    let view = Viewport::full(8200, 4);
    let plain = document.read_merged_region(view, &options);
    assert!(matches!(plain, Err(PsdError::CapExceeded { .. })));
    let region = document
        .read_merged_strided(view, 2, &options)
        .expect("the stepped output fits");
    assert_eq!(
        (region.viewport.width(), region.viewport.height()),
        (4100, 2)
    );
    assert_eq!(region.sample(0, 4099, 1), Some(77));
}

#[test]
fn a_step_of_zero_reads_every_pixel_and_a_bad_viewport_is_refused() {
    let document = document(8, 8, false);
    let options = ParseOptions::none();
    let region = document
        .read_merged_strided(Viewport::full(8, 8), 0, &options)
        .expect("a zero step acts as one");
    assert_eq!(region.viewport, Viewport::full(8, 8));
    let outside = document.read_merged_strided(Viewport::new(0, 0, 9, 8), 2, &options);
    assert!(matches!(outside, Err(PsdError::InvalidViewport { .. })));
}

#[test]
fn merged_pixels_convert_to_straight_rgba() {
    let document = document(6, 4, false);
    let region = document
        .read_merged_pixels(Viewport::full(6, 4), 2, &ParseOptions::none())
        .expect("pixels");
    assert_eq!((region.width(), region.height()), (3, 2));
    let rgba = region.rgba8();
    let expected = [
        pattern(6, 4, 0)[2],
        pattern(6, 4, 1)[2],
        pattern(6, 4, 2)[2],
        255,
    ];
    assert_eq!(rgba[4..8], expected);
}

/// A layered document whose merged section the parser cannot read.
fn layered_with_merged(section: Vec<u8>, with_layer: bool) -> Vec<u8> {
    let mut spec = DocumentSpec::new(8, 8, 8, 3);
    if with_layer {
        spec.layers
            .push(LayerSpec::opaque((0, 0, 8, 8), [10, 20, 30]));
    }
    spec.merged_section = Some(section);
    build_psd(&spec)
}

#[test]
fn a_bad_merged_section_fails_the_strict_open_but_not_the_lenient_one() {
    let mut zip = Vec::new();
    push_u16(&mut zip, 2);
    zip.extend_from_slice(&[0; 16]);
    let truncated = vec![0u8, 0, 1, 2];
    let dir = std::env::temp_dir().join(format!("tarragon-psd-lenient-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp folder");
    for (name, section) in [("zip", zip), ("short", truncated)] {
        let path = dir.join(format!("{name}.psd"));
        std::fs::write(&path, layered_with_merged(section, true)).expect("write");
        let options = ParseOptions::none();
        assert!(PsdDocument::open_with_options(&path, &options).is_err());
        let document = PsdDocument::open_lenient(&path, &options).expect("lenient opens");
        let file = document.file();
        assert!(file.merged_error.is_some(), "{name}");
        assert!(file.merged.channels.is_empty(), "{name}");
        assert_eq!(file.layers.len(), 1, "{name}");
    }
    // A file with no layers has nothing else to draw, so it still fails.
    let flat = dir.join("flat.psd");
    std::fs::write(&flat, layered_with_merged(vec![0, 2, 0, 0], false)).expect("write");
    assert!(PsdDocument::open_lenient(&flat, &ParseOptions::none()).is_err());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_good_file_opens_the_same_way_in_both_modes() {
    let bytes = build_psd(&DocumentSpec::new(4, 4, 8, 3));
    let dir = std::env::temp_dir().join(format!("tarragon-psd-same-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp folder");
    let path = dir.join("good.psd");
    std::fs::write(&path, bytes).expect("write");
    let options = ParseOptions::none();
    let strict = PsdDocument::open_with_options(&path, &options).expect("strict");
    let lenient = PsdDocument::open_lenient(&path, &options).expect("lenient");
    assert_eq!(strict.file(), lenient.file());
    assert!(lenient.file().merged_error.is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

/// Parse a file, then read its merged image from bytes that lack the data of
/// the last row of the last channel. The parsed structure still describes the
/// full file, so a read of that row fails.
fn skipped_tail_row_is_never_read(rle: bool) {
    let (width, height, step) = (6usize, 10usize, 4u32);
    let planes: Vec<Vec<u8>> = (0..3).map(|seed| pattern(width, height, seed)).collect();
    let mut spec = DocumentSpec::new(6, 10, 8, 3);
    let last_row = &planes[2][(height - 1) * width..];
    let tail = if rle {
        spec.merged_section = Some(rle_merged_section(width, &planes, false));
        common::packbits_row(last_row).len()
    } else {
        spec.merged = planes
            .iter()
            .map(|plane| plane.iter().map(|v| u16::from(*v)).collect())
            .collect();
        width
    };
    let data = build_psd(&spec);
    let file = tarragon_psd::parse::read_bytes(&data).expect("parses");
    let cut = &data[..data.len() - tail];
    let view = Viewport::full(6, 10);
    let options = ParseOptions::none();
    // Rows 0, 4 and 8 are kept. Row 9 is skipped and its bytes are gone.
    let kept = file
        .read_merged_strided(cut, view, step, &options)
        .expect("the skipped row is not read");
    let full = file
        .read_merged_strided(&data, view, step, &options)
        .expect("full");
    assert_eq!(kept, full);
    assert_eq!(
        kept.sample(2, 1, 2),
        Some(u16::from(planes[2][8 * width + step as usize]))
    );
    // A read of every row needs the missing bytes and fails.
    let plain = file.read_merged_region(cut, view, &options);
    assert!(
        matches!(plain, Err(PsdError::Truncated { .. })),
        "{plain:?}"
    );
}

#[test]
fn a_raw_stepped_read_does_not_read_the_skipped_rows() {
    skipped_tail_row_is_never_read(false);
}

#[test]
fn an_rle_stepped_read_does_not_read_the_skipped_rows() {
    skipped_tail_row_is_never_read(true);
}
