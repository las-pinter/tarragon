//! Color adjustment layers on synthetic PSD files: Exposure, Channel Mixer,
//! Color Balance, Hue/Saturation, Selective Color and Photo Filter.
//!
//! Each test states a rule that the adjustment follows, not a Photoshop
//! reference value.

mod common;

use common::{
    DocumentSpec, LayerSpec, blnc_block, build_psd, composite_bytes, expa_block, hue2_block,
    mixr_block, phfl_block, sample_rgba8, scaled, selc_block,
};

/// Composite one adjustment block over one pixel, as RGB8.
fn run(depth: u16, pixel: [u8; 3], key: &[u8], block: Vec<u8>) -> [u8; 3] {
    let mut spec = DocumentSpec::new(1, 1, depth, 3);
    let values = pixel.map(|value| scaled(u16::from(value), depth));
    spec.layers.push(LayerSpec::opaque((0, 0, 1, 1), values));
    spec.layers.push(
        LayerSpec::content((0, 0, 1, 1)).with_block(key.try_into().expect("four-byte key"), block),
    );
    let region = composite_bytes(build_psd(&spec));
    let out = sample_rgba8(&region, 0, 0);
    [out[0], out[1], out[2]]
}

fn run8(pixel: [u8; 3], key: &[u8], block: Vec<u8>) -> [u8; 3] {
    run(8, pixel, key, block)
}

/// The 2.2 power of an 8-bit level.
fn linear(level: u8) -> f32 {
    (f32::from(level) / 255.0).powf(2.2)
}

const NO_EDITS: [[i16; 3]; 6] = [[0; 3]; 6];

#[test]
fn exposure_minus_one_halves_linear_light() {
    let out = run8([200, 120, 60], b"expA", expa_block(-1.0, 0.0, 1.0));
    for (before, after) in [200u8, 120, 60].into_iter().zip(out) {
        let ratio = linear(after) / linear(before);
        assert!((ratio - 0.5).abs() < 0.03, "{before} -> {after}: {ratio}");
    }
}

#[test]
fn exposure_zero_is_the_identity() {
    let pixel = [200, 120, 60];
    assert_eq!(run8(pixel, b"expA", expa_block(0.0, 0.0, 1.0)), pixel);
}

#[test]
fn exposure_plus_one_doubles_linear_light_and_a_gamma_of_two_lifts_midtones() {
    let up = run8([100, 100, 100], b"expA", expa_block(1.0, 0.0, 1.0));
    let ratio = linear(up[0]) / linear(100);
    assert!((ratio - 2.0).abs() < 0.08, "{up:?} {ratio}");
    let lifted = run8([100, 100, 100], b"expA", expa_block(0.0, 0.0, 2.0));
    assert!(lifted[0] > 100, "{lifted:?}");
}

#[test]
fn a_huge_exposure_stays_finite_on_a_black_pixel() {
    // The offset 0.5 in linear light is level 186 after the 2.2 curve.
    let out = run8([0, 0, 0], b"expA", expa_block(1e30, 0.5, 1.0));
    assert!((i32::from(out[0]) - 186).abs() <= 1, "{out:?}");
    let clean = run8([0, 0, 0], b"expA", expa_block(1e30, 0.0, 1.0));
    assert_eq!(clean, [0, 0, 0]);
}

#[test]
fn a_monochrome_mixer_writes_the_green_mix_to_every_channel() {
    let block = mixr_block(true, [[0, 100, 0, 0], [0; 4], [0; 4]]);
    assert_eq!(
        run8([50, 150, 220], b"mixr", block),
        [150, 150, 150],
        "grey equals green"
    );
}

#[test]
fn a_color_mixer_swaps_channels_and_adds_a_constant() {
    // Red out takes blue. Green out stays. Blue out takes red plus 10 percent.
    let block = mixr_block(false, [[0, 0, 100, 0], [0, 100, 0, 0], [100, 0, 0, 10]]);
    let out = run8([50, 150, 200], b"mixr", block);
    assert_eq!(out[0], 200);
    assert_eq!(out[1], 150);
    assert!((i32::from(out[2]) - (50 + 26)).abs() <= 1, "{out:?}");
}

#[test]
fn a_zero_color_balance_is_the_identity() {
    let pixel = [30, 130, 220];
    for preserve in [false, true] {
        assert_eq!(
            run8(pixel, b"blnc", blnc_block([[0; 3]; 3], preserve)),
            pixel
        );
    }
}

#[test]
fn cyan_red_in_the_shadows_raises_red_on_a_dark_pixel() {
    let block = blnc_block([[100, 0, 0], [0; 3], [0; 3]], false);
    let dark = run8([40, 40, 40], b"blnc", block.clone());
    assert!(dark[0] > 100, "{dark:?}");
    assert_eq!((dark[1], dark[2]), (40, 40));
    // A bright pixel is out of the shadows and does not move.
    assert_eq!(run8([250, 250, 250], b"blnc", block), [250, 250, 250]);
}

#[test]
fn the_other_balance_axes_and_tones_move_their_channels() {
    // Magenta-green in the shadows raises green only.
    let shadows = blnc_block([[0, 100, 0], [0; 3], [0; 3]], false);
    let out = run8([40, 40, 40], b"blnc", shadows);
    assert!(out[1] > 100 && out[0] == 40 && out[2] == 40, "{out:?}");
    // Yellow-blue in the midtones raises blue on a mid grey.
    let mids = blnc_block([[0; 3], [0, 0, 100], [0; 3]], false);
    let out = run8([128, 128, 128], b"blnc", mids);
    assert!(out[2] > 128 && out[0] == 128 && out[1] == 128, "{out:?}");
    // Cyan-red in the highlights lowers red when negative on a light pixel.
    let lights = blnc_block([[0; 3], [0; 3], [-100, 0, 0]], false);
    let out = run8([230, 230, 230], b"blnc", lights);
    assert!(out[0] < 200 && out[1] == 230, "{out:?}");
}

#[test]
fn preserve_luminosity_keeps_the_grey_value_of_a_balanced_pixel() {
    let block = blnc_block([[0; 3], [0, 0, 80], [0; 3]], true);
    let out = run8([120, 120, 120], b"blnc", block);
    let grey = 0.299 * f32::from(out[0]) + 0.587 * f32::from(out[1]) + 0.114 * f32::from(out[2]);
    assert!((grey - 120.0).abs() < 1.5, "{out:?} {grey}");
    assert!(out[2] > out[0], "{out:?}");
}

#[test]
fn a_master_hue_of_180_rotates_a_red_to_cyan() {
    let out = run8(
        [200, 50, 50],
        b"hue2",
        hue2_block(None, [180, 0, 0], NO_EDITS),
    );
    assert!(out[0] < out[1] && out[0] < out[2], "{out:?}");
    assert!(
        (i32::from(out[1]) - i32::from(out[2])).abs() <= 1,
        "{out:?}"
    );
}

#[test]
fn a_neutral_hue_saturation_is_the_identity() {
    let pixel = [200, 90, 30];
    assert_eq!(
        run8(pixel, b"hue2", hue2_block(None, [0; 3], NO_EDITS)),
        pixel
    );
}

#[test]
fn colorize_sets_the_hue() {
    let block = hue2_block(Some([120, 100, 0]), [0; 3], NO_EDITS);
    let out = run8([200, 100, 100], b"hue2", block);
    assert!(out[1] > out[0] && out[1] > out[2], "{out:?}");
}

#[test]
fn a_reds_only_range_edit_moves_red_and_not_green() {
    let mut edits = NO_EDITS;
    edits[0] = [60, 0, 0];
    let block = hue2_block(None, [0; 3], edits);
    let red = run8([255, 0, 0], b"hue2", block.clone());
    assert!(red[1] > 150, "red turns toward yellow: {red:?}");
    assert_eq!(run8([0, 255, 0], b"hue2", block.clone()), [0, 255, 0]);
    // A grey has no hue and stays.
    assert_eq!(run8([128, 128, 128], b"hue2", block), [128, 128, 128]);
}

#[test]
fn a_reds_only_selective_edit_moves_red_and_not_green() {
    let mut records = [[0; 4]; 9];
    records[0] = [-50, 0, 0, 0];
    let block = selc_block(true, records);
    // Less cyan ink means more red.
    let red = run8([200, 60, 60], b"selc", block.clone());
    assert!(red[0] > 200, "{red:?}");
    assert_eq!(run8([0, 255, 0], b"selc", block), [0, 255, 0]);
}

#[test]
fn the_whites_neutrals_and_greens_records_follow_their_sign() {
    // Whites: more black ink in the whites darkens a light pixel, and leaves a dark one.
    let mut records = [[0; 4]; 9];
    records[6] = [0, 0, 0, 40];
    let block = selc_block(true, records);
    let light = run8([240, 240, 240], b"selc", block.clone());
    assert!(light[0] < 240, "{light:?}");
    assert_eq!(run8([20, 20, 20], b"selc", block), [20, 20, 20]);
    // Neutrals: a mid grey moves with the sign of the edit.
    records = [[0; 4]; 9];
    records[7] = [30, 0, 0, 0];
    let down = run8([128, 128, 128], b"selc", selc_block(true, records));
    assert!(down[0] < 128 && down[1] == 128, "{down:?}");
    records[7] = [-30, 0, 0, 0];
    let up = run8([128, 128, 128], b"selc", selc_block(true, records));
    assert!(up[0] > 128, "{up:?}");
    // Greens: yellow ink up in the greens lowers blue on a green pixel only.
    records = [[0; 4]; 9];
    records[2] = [0, 0, 50, 0];
    let block = selc_block(true, records);
    let green = run8([0, 255, 0], b"selc", block.clone());
    assert!(green[2] == 0 && green[1] == 255, "{green:?}");
    let green = run8([60, 200, 60], b"selc", block.clone());
    assert!(green[2] < 60, "{green:?}");
    assert_eq!(run8([255, 0, 0], b"selc", block), [255, 0, 0]);
}

#[test]
fn relative_and_absolute_selective_modes_differ() {
    let mut records = [[0; 4]; 9];
    records[0] = [0, 0, 0, 50];
    let relative = run8([200, 60, 60], b"selc", selc_block(false, records));
    let absolute = run8([200, 60, 60], b"selc", selc_block(true, records));
    assert_ne!(relative, absolute);
    // The black ink goes up in both, so the pixel gets darker in both.
    assert!(relative[0] < 200 && absolute[0] < 200);
    // Absolute mode moves further than relative mode.
    assert!(absolute[0] < relative[0], "{relative:?} {absolute:?}");
}

#[test]
fn a_photo_filter_of_zero_density_is_the_identity() {
    let pixel = [90, 140, 200];
    for preserve in [false, true] {
        assert_eq!(
            run8(pixel, b"phfl", phfl_block([236, 138, 0], 0, preserve)),
            pixel
        );
    }
}

#[test]
fn a_warm_photo_filter_raises_red_over_blue() {
    let out = run8(
        [128, 128, 128],
        b"phfl",
        phfl_block([236, 138, 0], 100, false),
    );
    assert!(out[0] > out[1] && out[1] > out[2], "{out:?}");
    assert!(out[0] > out[2] + 40, "{out:?}");
    // With luminosity kept, the red channel rises above the source grey.
    let kept = run8(
        [128, 128, 128],
        b"phfl",
        phfl_block([236, 138, 0], 100, true),
    );
    assert!(kept[0] > 128 && kept[2] < 128, "{kept:?}");
}

#[test]
fn a_version_three_photo_filter_composites() {
    // XYZ of a warm orange in D50, times 32768, at full density.
    let mut block = 3u16.to_be_bytes().to_vec();
    for value in [20_316i32, 16_384, 1_638] {
        block.extend_from_slice(&value.to_be_bytes());
    }
    block.extend_from_slice(&100u32.to_be_bytes());
    block.extend_from_slice(&[0, 0, 0, 0]);
    let out = run8([128, 128, 128], b"phfl", block.clone());
    assert!(out[0] > out[1] && out[1] > out[2], "{out:?}");
    let sixteen = run(16, [128, 128, 128], b"phfl", block);
    assert!(i32::from(out[0]).abs_diff(i32::from(sixteen[0])) <= 1);
}

#[test]
fn photo_filter_preserve_luminosity_keeps_the_brightness() {
    let free = run8(
        [128, 128, 128],
        b"phfl",
        phfl_block([0, 80, 255], 100, false),
    );
    let kept = run8(
        [128, 128, 128],
        b"phfl",
        phfl_block([0, 80, 255], 100, true),
    );
    let level =
        |p: [u8; 3]| 0.3 * f32::from(p[0]) + 0.59 * f32::from(p[1]) + 0.11 * f32::from(p[2]);
    assert!((level(kept) - 128.0).abs() < 2.0, "{kept:?}");
    assert!(level(free) < 120.0, "{free:?}");
}

#[test]
fn every_color_block_agrees_between_eight_and_sixteen_bit() {
    let mut edits = NO_EDITS;
    edits[0] = [30, 20, -10];
    let mut records = [[0; 4]; 9];
    records[0] = [-30, 20, 10, 5];
    records[7] = [10, 0, 0, 0];
    let blocks: [(&[u8; 4], Vec<u8>); 7] = [
        (b"expA", expa_block(0.7, 0.02, 1.4)),
        (
            b"mixr",
            mixr_block(false, [[80, 30, -10, 3], [10, 90, 0, 0], [0, 20, 80, 0]]),
        ),
        (
            b"blnc",
            blnc_block([[20, -30, 10], [40, 0, -20], [0, 10, 30]], true),
        ),
        (b"hue2", hue2_block(None, [25, -20, 10], edits)),
        (b"hue2", hue2_block(Some([200, 40, 5]), [0; 3], NO_EDITS)),
        (b"selc", selc_block(false, records)),
        (b"phfl", phfl_block([236, 138, 0], 60, true)),
    ];
    for pixel in [[180u8, 90, 60], [30, 160, 210], [120, 120, 130]] {
        for (key, block) in &blocks {
            let eight = run(8, pixel, *key, block.clone());
            let sixteen = run(16, pixel, *key, block.clone());
            for channel in 0..3 {
                let gap = (i32::from(eight[channel]) - i32::from(sixteen[channel])).abs();
                assert!(gap <= 1, "{key:?} {pixel:?}: {eight:?} vs {sixteen:?}");
            }
        }
    }
}
