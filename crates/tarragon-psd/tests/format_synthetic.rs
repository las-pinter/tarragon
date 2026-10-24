//! Format edge cases on synthetic files.
//!
//! These cover the layer info in a tagged block, PSB block lengths, the merged
//! transparency flag, grayscale, an unsupported layer, and the real mask
//! rectangle.

mod common;

use common::{DocumentSpec, LayerSpec, build_psd, composite_bytes, push_i32, sample_rgba8, scaled};
use tarragon_psd::{
    ColorMode, LayerBounds, LayerSection, ParseOptions, PsdDocument, PsdError, Viewport,
};

/// Parse owned bytes without limits.
fn open(bytes: Vec<u8>) -> PsdDocument {
    PsdDocument::from_bytes(bytes, &ParseOptions::none()).expect("the file parses")
}

/// A 2x2 document of two opaque layers.
fn two_layers(depth: u16) -> DocumentSpec {
    let mut spec = DocumentSpec::new(2, 2, depth, 3);
    spec.layers.push(LayerSpec::opaque(
        (0, 0, 2, 2),
        [scaled(100, depth), scaled(150, depth), scaled(200, depth)],
    ));
    let mut top = LayerSpec::opaque(
        (0, 0, 2, 2),
        [scaled(200, depth), scaled(50, depth), scaled(0, depth)],
    );
    top.opacity = 128;
    spec.layers.push(top);
    spec
}

#[test]
fn the_16bit_layer_block_supplies_the_layers() {
    let plain = composite_bytes(build_psd(&two_layers(16)));
    let mut spec = two_layers(16);
    spec.layer_info_block = Some(*b"Lr16");
    let bytes = build_psd(&spec);
    assert_eq!(open(bytes.clone()).file().layers.len(), 2);
    assert_eq!(composite_bytes(bytes).rgba8(), plain.rgba8());
}

#[test]
fn every_layer_info_block_key_is_read() {
    for key in [*b"Lr16", *b"Lr32", *b"Layr"] {
        let mut spec = two_layers(16);
        spec.layer_info_block = Some(key);
        let file = open(build_psd(&spec));
        assert_eq!(file.file().layers.len(), 2, "{key:?}");
    }
}

#[test]
fn a_psb_layer_info_block_uses_an_eight_byte_length() {
    let mut spec = two_layers(16);
    spec.psb = true;
    spec.layer_info_block = Some(*b"Lr16");
    assert_eq!(open(build_psd(&spec)).file().layers.len(), 2);
}

/// A group with a large-length block on its divider record.
fn group_with_block(key: [u8; 4], psb: bool) -> DocumentSpec {
    let mut spec = DocumentSpec::new(2, 2, 8, 3);
    spec.psb = psb;
    spec.layers.push(LayerSpec::opaque((0, 0, 2, 2), [9, 9, 9]));
    spec.layers
        .push(LayerSpec::group_marker(3, *b"pass").with_block(key, vec![0; 6]));
    spec.layers
        .push(LayerSpec::opaque((0, 0, 2, 2), [40, 50, 60]));
    spec.layers.push(LayerSpec::group_marker(1, *b"pass"));
    spec
}

#[test]
fn a_psb_large_length_block_does_not_hide_the_divider_after_it() {
    for key in [*b"PxSD", *b"FXid", *b"LMsk", *b"lnk2", *b"Alph"] {
        for psb in [true, false] {
            let bytes = build_psd(&group_with_block(key, psb));
            let document = open(bytes.clone());
            assert_eq!(
                document.file().layers[1].section,
                Some(LayerSection::BoundingSectionDivider),
                "{key:?} psb {psb}"
            );
            let pixel = sample_rgba8(&composite_bytes(bytes), 0, 0);
            assert_eq!(pixel, [40, 50, 60, 255], "{key:?} psb {psb}");
        }
    }
}

/// A one-layer RGB document with a fourth merged channel.
fn merged_with_extra(layers: bool, merged_alpha: bool) -> DocumentSpec {
    let mut spec = DocumentSpec::new(4, 1, 8, 3);
    if layers {
        spec.layers
            .push(LayerSpec::opaque((0, 0, 1, 4), [200, 100, 50]));
    }
    spec.merged_alpha = merged_alpha;
    spec.merged_channels = Some(4);
    // Colors, then a saved selection that is zero outside the selection.
    spec.merged = vec![
        vec![200; 4],
        vec![100; 4],
        vec![50; 4],
        vec![0, 0, 255, 255],
    ];
    spec
}

#[test]
fn a_saved_selection_channel_is_not_merged_transparency() {
    let region = composite_bytes(build_psd(&merged_with_extra(false, false)));
    for x in 0..4 {
        assert_eq!(sample_rgba8(&region, x, 0), [200, 100, 50, 255], "x {x}");
    }
}

#[test]
fn the_layer_count_sign_marks_the_merged_alpha_channel() {
    let flagged = open(build_psd(&merged_with_extra(true, true)));
    assert!(flagged.file().merged_has_alpha);
    let ids: Vec<i16> = flagged
        .file()
        .merged
        .channels
        .iter()
        .map(|c| c.id)
        .collect();
    assert_eq!(ids, [0, 1, 2, -1]);
    let plain = open(build_psd(&merged_with_extra(true, false)));
    assert!(!plain.file().merged_has_alpha);
    assert!(plain.file().merged.channel(-1).is_none());
    let region = plain
        .read_merged_region(Viewport::full(4, 1), &ParseOptions::none())
        .expect("reads");
    assert_eq!(region.channels.len(), 3);
}

#[test]
fn a_flattened_grayscale_file_composites_to_gray() {
    let mut spec = DocumentSpec::new(2, 1, 8, 1);
    spec.merged = vec![vec![10, 250]];
    let region = composite_bytes(build_psd(&spec));
    assert_eq!(sample_rgba8(&region, 0, 0), [10, 10, 10, 255]);
    assert_eq!(sample_rgba8(&region, 1, 0), [250, 250, 250, 255]);
}

#[test]
fn a_grayscale_merged_selection_channel_is_not_transparency() {
    let mut spec = DocumentSpec::new(2, 1, 8, 1);
    spec.merged_channels = Some(2);
    spec.merged = vec![vec![10, 250], vec![0, 0]];
    let region = composite_bytes(build_psd(&spec));
    assert_eq!(sample_rgba8(&region, 0, 0), [10, 10, 10, 255]);
}

#[test]
fn grayscale_layers_composite_at_both_depths() {
    for depth in [8u16, 16] {
        let mut spec = DocumentSpec::new(2, 2, depth, 1);
        spec.layers
            .push(LayerSpec::gray((0, 0, 2, 2), scaled(100, depth)));
        let mut top = LayerSpec::gray((0, 0, 2, 2), scaled(200, depth));
        top.channels.push((-1, vec![scaled(128, depth); 4]));
        spec.layers.push(top);
        let region = composite_bytes(build_psd(&spec));
        let pixel = sample_rgba8(&region, 0, 0);
        assert_eq!(pixel[0], pixel[1], "depth {depth}");
        assert_eq!(pixel[1], pixel[2], "depth {depth}");
        assert!((i32::from(pixel[0]) - 150).abs() <= 1, "{pixel:?}");
        assert_eq!(pixel[3], 255);
    }
}

#[test]
fn grayscale_has_one_color_channel() {
    assert_eq!(ColorMode::Grayscale.color_channels(), Some(1));
}

#[test]
fn a_layer_with_unsupported_compression_is_skipped() {
    let mut spec = DocumentSpec::new(2, 2, 8, 3);
    spec.layers
        .push(LayerSpec::opaque((0, 0, 2, 2), [200, 100, 50]));
    let mut zipped = LayerSpec::opaque((0, 0, 2, 2), [0, 0, 0]);
    zipped.compression = 2;
    spec.layers.push(zipped);
    let region = composite_bytes(build_psd(&spec));
    assert_eq!(sample_rgba8(&region, 0, 0), [200, 100, 50, 255]);
    assert_eq!(region.skipped_layers(), 1);
    // A render with no skipped layer reports zero.
    let plain = composite_bytes(build_psd(&two_layers(8)));
    assert_eq!(plain.skipped_layers(), 0);
}

#[test]
fn a_corrupt_trailing_block_does_not_fail_a_flattened_file() {
    let mut spec = DocumentSpec::new(2, 1, 8, 3);
    spec.merged = vec![vec![10, 10], vec![20, 20], vec![30, 30]];
    // A layer info block that claims far more bytes than the section holds.
    let mut tail = b"8BIMLr16".to_vec();
    tail.extend_from_slice(&0x7FFF_FF00u32.to_be_bytes());
    spec.section_tail = tail;
    let bytes = build_psd(&spec);
    let document = open(bytes.clone());
    assert!(document.layer_info_error().is_some());
    let region = composite_bytes(bytes);
    assert_eq!(sample_rgba8(&region, 0, 0), [10, 20, 30, 255]);
    // A clean file reports no dropped error.
    assert!(open(build_psd(&two_layers(8))).layer_info_error().is_none());
}

#[test]
fn an_unsupported_color_mode_still_fails_the_render() {
    let mut spec = DocumentSpec::new(2, 2, 8, 9);
    spec.layers.push(LayerSpec::opaque((0, 0, 2, 2), [1, 2, 3]));
    let document = open(build_psd(&spec));
    let result = document.composite_region(Viewport::full(2, 2), &ParseOptions::none());
    assert!(matches!(result, Err(PsdError::UnsupportedColorMode { .. })));
}

/// Push four edges: top, left, bottom, right.
fn push_rect(body: &mut Vec<u8>, rect: [i32; 4]) {
    for edge in rect {
        push_i32(body, edge);
    }
}

/// A one-layer document whose mask body is `body`.
fn masked(body: Vec<u8>) -> PsdDocument {
    let mut spec = DocumentSpec::new(2, 2, 8, 3);
    spec.layers
        .push(LayerSpec::opaque((0, 0, 2, 2), [9, 9, 9]).with_raw_mask(body));
    open(build_psd(&spec))
}

#[test]
fn mask_parameters_are_skipped_before_the_real_rectangle() {
    let mut body = Vec::new();
    push_rect(&mut body, [0, 0, 2, 2]);
    // The default color, then the flags with the parameters bit.
    body.extend_from_slice(&[255, 0x10]);
    // Presence bits for a user density and a user feather, then both values.
    // The feather bytes would read as a huge rectangle at the fixed offset.
    body.extend_from_slice(&[0x03, 128, 0x7F, 0xFF, 0xFF, 0xFF, 0x80, 0, 0, 0]);
    // The real flags, the real background, and the real rectangle.
    body.extend_from_slice(&[0, 255]);
    push_rect(&mut body, [0, 0, 2, 2]);
    let document = masked(body);
    let mask = document.file().layers[0].mask.expect("a mask");
    assert_eq!(
        mask.real_bounds,
        Some(LayerBounds {
            top: 0,
            left: 0,
            bottom: 2,
            right: 2
        })
    );
}

#[test]
fn a_bad_real_rectangle_does_not_fail_the_file() {
    let mut body = Vec::new();
    push_rect(&mut body, [0, 0, 2, 2]);
    body.extend_from_slice(&[255, 0, 0, 255]);
    push_rect(&mut body, [0, 0, 2, i32::MAX]);
    let document = masked(body);
    let mask = document.file().layers[0].mask.expect("a mask");
    assert_eq!(mask.real_bounds, None);
    assert_eq!(mask.default_color, 255);
}
