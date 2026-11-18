//! PSD and PSB compositor behavior on the committed fixture corpus.
//!
//! The fixture tests read the committed corpus in `tests/fixtures/psd/`.
//!
//! These tests use the generator's known pixel formulas.

mod common;

use common::{composite_fixture, sample_rgba8};

// --------------------------------------------------------------------------
// Fixture tests
// --------------------------------------------------------------------------

#[test]
fn the_layer_groups_fixture_composites_its_pass_through_children() {
    // Background (30,30,30), then a pass-through group: Child A (200,80,40)
    // and Child B (40,120,200). Child B covers Child A.
    let region = composite_fixture("layer_groups.psd");
    assert_eq!(region.width(), 96);
    assert_eq!(sample_rgba8(&region, 0, 0), [40, 120, 200, 255]);
    assert_eq!(sample_rgba8(&region, 95, 95), [40, 120, 200, 255]);
}

#[test]
fn the_clipping_fixture_restricts_to_the_base() {
    // Base (200,200,200); Clip A (220,60,60) full canvas; Clip B (60,60,220)
    // only in the top-left 48x48. Both clip to the base alpha.
    let region = composite_fixture("clipping_mask.psd");
    assert_eq!(sample_rgba8(&region, 10, 10), [60, 60, 220, 255]);
    assert_eq!(sample_rgba8(&region, 80, 80), [220, 60, 60, 255]);
    // The clip boundary is at x = 48.
    assert_eq!(sample_rgba8(&region, 47, 10), [60, 60, 220, 255]);
    assert_eq!(sample_rgba8(&region, 48, 10), [220, 60, 60, 255]);
}

#[test]
fn the_layer_mask_fixture_cuts_the_layer_alpha() {
    // The mask channel is ((x*4) & 0xFF) / 255. The layer color is
    // (120,180,90) with alpha 255, so the mask scales the output alpha.
    let region = composite_fixture("layer_mask.psd");
    let left = region.alpha(0, 0).expect("alpha");
    assert!(left.abs() < 1e-6, "the mask is black at x = 0");
    let middle = region.alpha(16, 0).expect("alpha");
    assert!((middle - 64.0 / 255.0).abs() < 1e-6, "mask at x = 16");
    let right = region.alpha(63, 0).expect("alpha");
    assert!((right - 252.0 / 255.0).abs() < 1e-6, "mask at x = 63");
    // The masked color is the layer color where the mask is not black.
    let color = region.color(16, 0).expect("color");
    assert!((color[0] - 120.0 / 255.0).abs() < 1e-6);
    assert!((color[1] - 180.0 / 255.0).abs() < 1e-6);
    assert!((color[2] - 90.0 / 255.0).abs() < 1e-6);
}

#[test]
fn the_cmyk_fixture_decodes_to_rgb() {
    // The layer stores CMYK (200,40,120,20). PSD stores the samples inverted,
    // so the ink is (55,215,135,235). The naive conversion is
    // R = (1-C)*(1-K), G = (1-M)*(1-K), B = (1-Y)*(1-K).
    let region = composite_fixture("cmyk_8bit.psd");
    assert_eq!(sample_rgba8(&region, 0, 0), [16, 3, 9, 255]);
    assert_eq!(sample_rgba8(&region, 63, 63), [16, 3, 9, 255]);
}
