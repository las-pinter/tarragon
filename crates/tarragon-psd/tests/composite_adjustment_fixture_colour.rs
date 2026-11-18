//! Rule tests for the last five cells of the committed adjustment fixture.
//!
//! Each test states a rule about one adjustment block, not a Photoshop value.
//! The rules use order and sign, so they hold under the recorded tint
//! approximations. They do not prove Photoshop parity. The expected numbers
//! come from the rule text, not from a compositor run.

mod common;

use common::{adjustment_cell, composite_fixture};

/// Rule: master hue 180 turns (200, 50, 50) into a pixel whose red is below its
/// green and below its blue, and whose green and blue differ by 1 or less.
#[test]
fn the_hue_cell_turns_red_into_cyan() {
    let region = composite_fixture("adjustment_blocks.psd");
    let [red, green, blue] = adjustment_cell(&region, 5);
    assert!(red < green, "{red} {green} {blue}");
    assert!(red < blue, "{red} {green} {blue}");
    assert!(green.abs_diff(blue) <= 1, "{red} {green} {blue}");
}

/// Rule: cyan-red +100 in the shadows turns (40, 40, 40) into a pixel with red
/// above 100 and green and blue still 40.
#[test]
fn the_balance_cell_pushes_the_shadows_toward_red() {
    let region = composite_fixture("adjustment_blocks.psd");
    let [red, green, blue] = adjustment_cell(&region, 6);
    assert!(red > 100, "{red} {green} {blue}");
    assert_eq!([green, blue], [40, 40], "{red} {green} {blue}");
}

/// Rule: red out takes blue, green out stays, and blue out takes red plus 10
/// percent. On (50, 150, 200) the pixel is red 200, green 150, and blue 76
/// within 1 (50 plus 26, the 10 percent of 255 rounded).
#[test]
fn the_mixer_cell_swaps_channels_and_adds_a_constant() {
    let region = composite_fixture("adjustment_blocks.psd");
    let [red, green, blue] = adjustment_cell(&region, 7);
    assert_eq!(red, 200, "{red} {green} {blue}");
    assert_eq!(green, 150, "{red} {green} {blue}");
    assert!(blue.abs_diff(76) <= 1, "{red} {green} {blue}");
}

/// Rule: cyan -50 in the reds (absolute) means less cyan ink, so red rises:
/// (200, 60, 60) becomes a pixel with red above 200.
#[test]
fn the_selective_colour_cell_raises_red() {
    let region = composite_fixture("adjustment_blocks.psd");
    let [red, green, blue] = adjustment_cell(&region, 8);
    assert!(red > 200, "{red} {green} {blue}");
}

/// Rule: a warm filter at density 100, luminosity not kept, turns
/// (128, 128, 128) into a pixel with red above green, green above blue, and red
/// above blue by more than 40.
#[test]
fn the_photo_filter_cell_warms_the_grey() {
    let region = composite_fixture("adjustment_blocks.psd");
    let [red, green, blue] = adjustment_cell(&region, 9);
    assert!(red > green, "{red} {green} {blue}");
    assert!(green > blue, "{red} {green} {blue}");
    assert!(red.abs_diff(blue) > 40, "{red} {green} {blue}");
}
