//! Vector mask (`vmsk`, `vsms`) behavior on synthetic in-memory files.
//!
//! Each scene is an 8 by 8 canvas: an opaque black base and a white layer on
//! top that carries the vector mask. Inside the mask the pixel is white and
//! outside it shows the black base.

mod common;

use common::{
    DocumentSpec, LayerSpec, VMSK_DISABLED, VMSK_INVERT, build_psd, composite, composite_bytes,
    ellipse_path, initial_fill_record, rect_path, sample_rgba8, stitched_rgba8, vmsk_block,
};
use tarragon_psd::{ParseOptions, PsdDocument, Viewport};

const WHITE: [u8; 4] = [255, 255, 255, 255];
const BLACK: [u8; 4] = [0, 0, 0, 255];

/// A canvas with a black base and a white layer that carries `layer_edit`.
fn scene(edit: impl FnOnce(LayerSpec) -> LayerSpec) -> Vec<u8> {
    let mut spec = DocumentSpec::new(8, 8, 8, 3);
    spec.layers.push(LayerSpec::opaque((0, 0, 8, 8), [0, 0, 0]));
    spec.layers
        .push(edit(LayerSpec::opaque((0, 0, 8, 8), [255, 255, 255])));
    build_psd(&spec)
}

/// A white layer masked by a centered 4 by 4 square (pixels 2 to 5).
fn square_mask(flags: u32) -> Vec<u8> {
    vmsk_block(flags, &[rect_path(1, 0.25, 0.25, 0.75, 0.75)])
}

#[test]
fn a_vector_mask_clips_its_layer() {
    let region = composite_bytes(scene(|l| l.with_block(*b"vmsk", square_mask(0))));
    assert_eq!(sample_rgba8(&region, 3, 3), WHITE);
    assert_eq!(sample_rgba8(&region, 2, 5), WHITE);
    assert_eq!(sample_rgba8(&region, 1, 3), BLACK);
    assert_eq!(sample_rgba8(&region, 6, 3), BLACK);
    assert_eq!(sample_rgba8(&region, 0, 0), BLACK);
}

#[test]
fn the_vsms_key_acts_like_vmsk() {
    let a = composite_bytes(scene(|l| l.with_block(*b"vmsk", square_mask(0))));
    let b = composite_bytes(scene(|l| l.with_block(*b"vsms", square_mask(0))));
    assert_eq!(a.rgba8(), b.rgba8());
}

#[test]
fn an_inverted_vector_mask_clips_the_inside() {
    let region = composite_bytes(scene(|l| l.with_block(*b"vmsk", square_mask(VMSK_INVERT))));
    assert_eq!(sample_rgba8(&region, 3, 3), BLACK);
    assert_eq!(sample_rgba8(&region, 0, 0), WHITE);
}

#[test]
fn a_disabled_vector_mask_does_nothing() {
    let region = composite_bytes(scene(|l| {
        l.with_block(*b"vmsk", square_mask(VMSK_DISABLED))
    }));
    assert_eq!(sample_rgba8(&region, 0, 0), WHITE);
    assert_eq!(sample_rgba8(&region, 3, 3), WHITE);
}

#[test]
fn the_raster_mask_and_the_vector_mask_multiply() {
    // A flat raster mask of 128 halves the white inside the square.
    let region = composite_bytes(scene(|l| {
        l.with_mask((0, 0, 8, 8), 128)
            .with_block(*b"vmsk", square_mask(0))
    }));
    let inside = sample_rgba8(&region, 3, 3);
    assert!(inside[0].abs_diff(128) <= 1, "{inside:?}");
    assert_eq!(sample_rgba8(&region, 0, 0), BLACK);
}

#[test]
fn a_group_with_only_a_vector_mask_is_masked() {
    let mut spec = DocumentSpec::new(8, 8, 8, 3);
    spec.layers.push(LayerSpec::opaque((0, 0, 8, 8), [0, 0, 0]));
    spec.layers.push(LayerSpec::group_marker(3, *b"norm"));
    spec.layers
        .push(LayerSpec::opaque((0, 0, 8, 8), [255, 255, 255]));
    spec.layers
        .push(LayerSpec::group_marker(1, *b"norm").with_block(*b"vmsk", square_mask(0)));
    let region = composite_bytes(build_psd(&spec));
    assert_eq!(sample_rgba8(&region, 3, 3), WHITE);
    assert_eq!(sample_rgba8(&region, 0, 0), BLACK);
}

#[test]
fn a_curved_path_has_partial_coverage_on_its_edge() {
    let circle = vmsk_block(0, &[ellipse_path(1, 0.5, 0.5, 0.375, 0.375)]);
    let region = composite_bytes(scene(|l| l.with_block(*b"vmsk", circle)));
    assert_eq!(sample_rgba8(&region, 3, 3), WHITE);
    assert_eq!(sample_rgba8(&region, 0, 0), BLACK);
    let edge = sample_rgba8(&region, 1, 3);
    assert!(edge[0] > 0 && edge[0] < 255, "edge pixel {edge:?}");
    let corner = sample_rgba8(&region, 1, 1);
    assert!(corner[0] > 0 && corner[0] < 255, "corner pixel {corner:?}");
}

#[test]
fn a_subtract_path_cuts_a_hole() {
    let block = vmsk_block(
        0,
        &[
            rect_path(1, 0.0, 0.0, 1.0, 1.0),
            rect_path(2, 0.25, 0.25, 0.75, 0.75),
        ],
    );
    let region = composite_bytes(scene(|l| l.with_block(*b"vmsk", block)));
    assert_eq!(sample_rgba8(&region, 0, 0), WHITE);
    assert_eq!(sample_rgba8(&region, 3, 3), BLACK);
}

#[test]
fn the_initial_fill_starts_with_everything_covered() {
    let block = vmsk_block(
        0,
        &[
            initial_fill_record(true),
            rect_path(1, 0.0, 0.0, 0.25, 0.25),
            rect_path(2, 0.25, 0.25, 0.75, 0.75),
        ],
    );
    let region = composite_bytes(scene(|l| l.with_block(*b"vmsk", block)));
    assert_eq!(sample_rgba8(&region, 7, 7), WHITE);
    assert_eq!(sample_rgba8(&region, 3, 3), BLACK);
}

#[test]
fn block_padding_is_accepted() {
    let mut block = square_mask(0);
    block.extend_from_slice(&[0, 0]);
    let region = composite_bytes(scene(|l| l.with_block(*b"vmsk", block)));
    assert_eq!(sample_rgba8(&region, 0, 0), BLACK);
    assert_eq!(sample_rgba8(&region, 3, 3), WHITE);
}

#[test]
fn a_truncated_block_never_panics_and_keeps_the_whole_subpaths() {
    // Two rectangles: the first covers the left half, the second the top.
    let first = rect_path(1, 0.0, 0.0, 0.5, 1.0);
    let split = first.len();
    let full = vmsk_block(0, &[first, rect_path(1, 0.0, 0.0, 1.0, 0.5)]);
    for cut in 0..full.len() {
        let block = full[..cut].to_vec();
        let region = composite_bytes(scene(|l| l.with_block(*b"vmsk", block)));
        let left_bottom = sample_rgba8(&region, 0, 7);
        let right_bottom = sample_rgba8(&region, 7, 7);
        let right_top = sample_rgba8(&region, 7, 0);
        if cut < 8 + split {
            // Header problem or nothing recovered: the layer stays unmasked,
            // except a clean header with padding only, which covers nothing.
            if !(8..12).contains(&cut) {
                assert_eq!(left_bottom, WHITE, "cut at {cut}");
            }
        } else {
            // The first rectangle survives. The cut one never adds area.
            assert_eq!(left_bottom, WHITE, "cut at {cut}");
            assert_eq!(right_bottom, BLACK, "cut at {cut}");
            if cut < full.len() - 3 {
                assert_eq!(right_top, BLACK, "cut at {cut}");
            }
        }
    }
}

#[test]
fn a_tile_equals_the_full_render() {
    let circle = vmsk_block(0, &[ellipse_path(1, 0.5, 0.5, 0.4, 0.3)]);
    let bytes = scene(|l| l.with_block(*b"vmsk", circle));
    let full = composite(bytes.clone(), Viewport::full(8, 8));
    let document = PsdDocument::from_bytes(bytes, &ParseOptions::none()).expect("parses");
    let tiles = stitched_rgba8(&document, 8, 8, 3);
    assert_eq!(tiles, full.rgba8());
}
