//! Rule tests for the first five cells of the committed adjustment fixture.
//!
//! Each test states a rule about one adjustment block, not a Photoshop value.
//! The expected numbers come from the rule text, not from a compositor run.

mod common;

use common::{adjustment_cell, composite_fixture, sample_rgba8};

/// The 2.2 power of an 8-bit level.
fn linear(level: u8) -> f32 {
    (f32::from(level) / 255.0).powf(2.2)
}

/// Whether each channel of `pixel` is within `tolerance` of `expected`.
fn near(pixel: [u8; 3], expected: [u8; 3], tolerance: u8) -> bool {
    pixel
        .iter()
        .zip(expected)
        .all(|(got, want)| got.abs_diff(want) <= tolerance)
}

/// Rule: exposure +1 doubles linear light, so the ratio of linear light after
/// to before is 2.0 within 0.08 on each channel.
#[test]
fn the_exposure_cell_doubles_linear_light() {
    let region = composite_fixture("adjustment_blocks.psd");
    let pixel = adjustment_cell(&region, 0);
    for level in pixel {
        let ratio = linear(level) / linear(100);
        assert!((ratio - 2.0).abs() < 0.08, "{pixel:?} {ratio}");
    }
}

/// Rule: a vibrance block with saturation -100 removes all colour, so the
/// spread of the pixel is 1 or less.
#[test]
fn the_vibrance_cell_removes_the_colour() {
    let region = composite_fixture("adjustment_blocks.psd");
    let pixel = adjustment_cell(&region, 1);
    let spread = pixel.iter().max().expect("max") - pixel.iter().min().expect("min");
    assert!(spread <= 1, "{pixel:?}");
}

/// Rule: black and white with the default weights turns pure red into grey 102
/// (255 times the red weight 0.40).
#[test]
fn the_black_and_white_cell_weights_pure_red() {
    let region = composite_fixture("adjustment_blocks.psd");
    let pixel = adjustment_cell(&region, 2);
    assert!(near(pixel, [102, 102, 102], 1), "{pixel:?}");
}

/// Rule: a reversed black-to-white gradient map turns luminance 64 into 191
/// (255 minus 64, the ramp reversed).
#[test]
fn the_gradient_map_cell_reverses_the_ramp() {
    let region = composite_fixture("adjustment_blocks.psd");
    let pixel = adjustment_cell(&region, 3);
    assert!(near(pixel, [191, 191, 191], 1), "{pixel:?}");
}

/// Rule: a lookup cube that swaps red and blue turns (200, 100, 20) into
/// (20, 100, 200) exactly.
#[test]
fn the_lookup_cell_swaps_red_and_blue() {
    let region = composite_fixture("adjustment_blocks.psd");
    assert_eq!(adjustment_cell(&region, 4), [20, 100, 200]);
}

/// Rule: each layer mask keeps its block in its own cell, so the canvas
/// corner, the margin, the far gutter and the gutter between cells 0 and 1
/// keep the base grey.
#[test]
fn the_gutters_and_the_base_keep_their_colour() {
    let region = composite_fixture("adjustment_blocks.psd");
    for (x, y) in [(0, 0), (2, 22), (102, 42), (22, 12)] {
        let pixel = sample_rgba8(&region, x, y);
        assert_eq!(pixel[..3], [128, 128, 128], "({x}, {y})");
    }
}
