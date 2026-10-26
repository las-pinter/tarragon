//! Vibrance, Black & White and Gradient Map layers on synthetic PSD files.
//!
//! Each test states a rule that the adjustment follows, not a Photoshop
//! reference value.

mod common;

use common::{
    DocumentSpec, LayerSpec, build_psd, composite_bytes, descriptor_body, descriptor_item, rgbc,
    sample_rgba8, scaled, value_boolean, value_descriptor, value_integer, versioned,
};

/// Composite one adjustment block over one pixel, as RGB8.
fn run(depth: u16, pixel: [u8; 3], key: [u8; 4], block: Vec<u8>) -> [u8; 3] {
    let mut spec = DocumentSpec::new(1, 1, depth, 3);
    let values = pixel.map(|value| scaled(u16::from(value), depth));
    spec.layers.push(LayerSpec::opaque((0, 0, 1, 1), values));
    spec.layers
        .push(LayerSpec::content((0, 0, 1, 1)).with_block(key, block));
    let region = composite_bytes(build_psd(&spec));
    let out = sample_rgba8(&region, 0, 0);
    [out[0], out[1], out[2]]
}

fn run8(pixel: [u8; 3], key: [u8; 4], block: Vec<u8>) -> [u8; 3] {
    run(8, pixel, key, block)
}

fn items(entries: &[(&[u8], Vec<u8>)]) -> Vec<Vec<u8>> {
    entries
        .iter()
        .map(|(key, value)| descriptor_item(key, value))
        .collect()
}

/// A `vibA` block. `None` leaves the key out, as Photoshop does for zero.
fn viba(vibrance: Option<i32>, saturation: Option<i32>) -> Vec<u8> {
    let mut entries: Vec<(&[u8], Vec<u8>)> = Vec::new();
    if let Some(value) = vibrance {
        entries.push((b"vibrance", value_integer(value)));
    }
    if let Some(value) = saturation {
        entries.push((b"Strt", value_integer(value)));
    }
    versioned(&descriptor_body(b"null", &items(&entries)))
}

/// A `blwh` block with the six weights (red, yellow, green, cyan, blue, magenta).
fn blwh(weights: [i32; 6], tint: Option<[f64; 3]>) -> Vec<u8> {
    let keys: [&[u8]; 6] = [b"Rd  ", b"Yllw", b"Grn ", b"Cyn ", b"Bl  ", b"Mgnt"];
    let mut entries: Vec<(&[u8], Vec<u8>)> = keys
        .into_iter()
        .zip(weights)
        .map(|(key, weight)| (key, value_integer(weight)))
        .collect();
    entries.push((b"useTint", value_boolean(tint.is_some())));
    if let Some([red, green, blue]) = tint {
        entries.push((b"tintColor", value_descriptor(&rgbc(red, green, blue))));
    }
    versioned(&descriptor_body(b"null", &items(&entries)))
}

const DEFAULT_WEIGHTS: [i32; 6] = [40, 60, 40, 60, 20, 80];

/// A `grdm` block. Each stop is a location out of 4096, a midpoint percent
/// and an 8-bit color.
fn grdm(reverse: bool, stops: &[(u32, u32, [u8; 3])]) -> Vec<u8> {
    let mut data = 1u16.to_be_bytes().to_vec();
    data.extend_from_slice(&[u8::from(reverse), 0]);
    data.extend_from_slice(&0u32.to_be_bytes());
    data.extend_from_slice(&u16::try_from(stops.len()).expect("count").to_be_bytes());
    for (location, midpoint, rgb) in stops {
        data.extend_from_slice(&location.to_be_bytes());
        data.extend_from_slice(&midpoint.to_be_bytes());
        data.extend_from_slice(&0u16.to_be_bytes());
        for value in rgb {
            data.extend_from_slice(&(u16::from(*value) * 257).to_be_bytes());
        }
        // The fourth component and the pad.
        data.extend_from_slice(&[0; 4]);
    }
    // Two transparency stops, then the expansion count and smoothness.
    data.extend_from_slice(&2u16.to_be_bytes());
    for location in [0u32, 4096] {
        data.extend_from_slice(&location.to_be_bytes());
        data.extend_from_slice(&50u32.to_be_bytes());
        data.extend_from_slice(&0x7fffu16.to_be_bytes());
    }
    data.extend_from_slice(&2u16.to_be_bytes());
    data.extend_from_slice(&4096u16.to_be_bytes());
    data
}

/// The same block as `grdm`, written as version 3 with an interpolation method.
fn grdm_v3(stops: &[(u32, u32, [u8; 3])]) -> Vec<u8> {
    let mut data = grdm(false, stops);
    data[1] = 3;
    data.splice(4..4, *b"Lnr ");
    data
}

fn black_to_white() -> Vec<(u32, u32, [u8; 3])> {
    vec![(0, 50, [0; 3]), (4096, 50, [255; 3])]
}

fn spread(pixel: [u8; 3]) -> i32 {
    i32::from(pixel.into_iter().max().unwrap_or(0))
        - i32::from(pixel.into_iter().min().unwrap_or(0))
}

fn close(a: [u8; 3], b: [u8; 3], tolerance: i32) -> bool {
    a.into_iter()
        .zip(b)
        .all(|(x, y)| (i32::from(x) - i32::from(y)).abs() <= tolerance)
}

#[test]
fn vibrance_zero_zero_is_the_identity() {
    for pixel in [[200, 120, 60], [10, 10, 10], [250, 250, 250]] {
        assert!(close(run8(pixel, *b"vibA", viba(None, None)), pixel, 1));
        assert!(close(
            run8(pixel, *b"vibA", viba(Some(0), Some(0))),
            pixel,
            1
        ));
    }
}

#[test]
fn positive_saturation_raises_the_chroma_of_a_mid_saturated_pixel() {
    let pixel = [160, 120, 90];
    let out = run8(pixel, *b"vibA", viba(None, Some(50)));
    assert!(spread(out) > spread(pixel) + 10, "{out:?}");
}

#[test]
fn negative_saturation_of_one_hundred_gives_grey() {
    for pixel in [[200, 80, 40], [30, 160, 210]] {
        let out = run8(pixel, *b"vibA", viba(None, Some(-100)));
        assert!(spread(out) <= 1, "{pixel:?} -> {out:?}");
    }
}

#[test]
fn saturation_pins_to_linear_light_and_the_red_green_grey() {
    // Blue has no weight in the grey, so full desaturation sends pure blue to black.
    let blue = run8([0, 0, 255], *b"vibA", viba(None, Some(-100)));
    assert!(close(blue, [0, 0, 0], 2), "{blue:?}");
    // [200, 80, 40] decodes to linear (0.57758, 0.08022, 0.02122). The grey is
    // 0.288 * 0.57758 + 0.712 * 0.08022 = 0.22346. Strt -50 halves the distance
    // to it: (0.40052, 0.15184, 0.12234). Encoded again that is (170, 109, 98).
    let out = run8([200, 80, 40], *b"vibA", viba(None, Some(-50)));
    assert!(close(out, [170, 109, 98], 2), "{out:?}");
}

#[test]
fn vibrance_runs_before_saturation_in_linear_light() {
    // [140, 110, 90] decodes to linear (0.26225, 0.15593, 0.10224), so
    // t = 1 - min / max = 0.61014 and V = 0.26225. The hue is 20.1 degrees, a
    // skin hue, so for v = 0.5: a = 0.64 * 0.5^1.6 = 0.21112. The boost is
    // (0.5001 a + 0.0887 a^2) t^0.4374 (1-t)^0.7573 exp((-0.8113 + 0.5154 a
    // + 2.8026 (1-t)) (1-V)) = 0.05766, so t' = 0.64532. The shape term is
    // 0.08110, so V' = V + v / 4 * 0.08110 = 0.27239. Then min' = V' (1 - t')
    // = 0.09661 and k = (V' - min') / (V - min) = 1.09855, giving linear
    // (0.27239, 0.15559, 0.09661). Saturation 50 uses the grey
    // 0.288 R + 0.712 G = 0.18922 and scales by 1.5: (0.31397, 0.13877,
    // 0.05030). Encoded that is (152, 104, 63).
    let out = run8([140, 110, 90], *b"vibA", viba(Some(50), Some(50)));
    assert!(close(out, [152, 104, 63], 2), "{out:?}");
}

#[test]
fn positive_vibrance_helps_a_muted_pixel_more_than_a_vivid_one() {
    let gain = |pixel: [u8; 3]| {
        let out = run8(pixel, *b"vibA", viba(Some(80), None));
        f64::from(spread(out)) / f64::from(spread(pixel))
    };
    let muted = gain([120, 130, 105]);
    let vivid = gain([230, 30, 40]);
    assert!(muted > 1.1, "muted gain {muted}");
    assert!(muted > vivid, "muted {muted} vivid {vivid}");
}

#[test]
fn black_and_white_defaults_turn_red_into_forty_percent_grey() {
    let out = run8([255, 0, 0], *b"blwh", blwh(DEFAULT_WEIGHTS, None));
    assert!(close(out, [102, 102, 102], 1), "{out:?}");
    // Yellow is the sum of the yellow weight and no primary part.
    let yellow = run8([255, 255, 0], *b"blwh", blwh(DEFAULT_WEIGHTS, None));
    assert!(close(yellow, [153, 153, 153], 1), "{yellow:?}");
}

#[test]
fn black_and_white_gives_each_hue_its_own_weight() {
    // Weights for red, yellow, green, cyan, blue, magenta; all distinct.
    let weights = [20, 40, 60, 80, 100, 10];
    let level = |percent: i32| f64::from(percent) * 2.55;
    // A pure primary or secondary has max 1 and mid 0 or 1, so grey is its weight.
    let pure: [([u8; 3], i32); 6] = [
        ([255, 0, 0], weights[0]),
        ([255, 255, 0], weights[1]),
        ([0, 255, 0], weights[2]),
        ([0, 255, 255], weights[3]),
        ([0, 0, 255], weights[4]),
        ([255, 0, 255], weights[5]),
    ];
    for (pixel, weight) in pure {
        let out = run8(pixel, *b"blwh", blwh(weights, None));
        assert!(
            (f64::from(out[0]) - level(weight)).abs() <= 1.0,
            "{pixel:?}: {out:?}"
        );
    }
    // One pixel per hue sector: max 1, mid 0.4, min 0. By hand the grey is
    // 0.4 * secondary weight + 0.6 * primary weight.
    let mixed: [([u8; 3], usize, usize); 6] = [
        ([255, 102, 0], 1, 0),
        ([102, 255, 0], 1, 2),
        ([0, 255, 102], 3, 2),
        ([0, 102, 255], 3, 4),
        ([255, 0, 102], 5, 0),
        ([102, 0, 255], 5, 4),
    ];
    for (pixel, secondary, primary) in mixed {
        let expected = 0.4 * level(weights[secondary]) + 0.6 * level(weights[primary]);
        let out = run8(pixel, *b"blwh", blwh(weights, None));
        assert!(
            (f64::from(out[0]) - expected).abs() <= 1.0,
            "{pixel:?}: {out:?} vs {expected}"
        );
    }
}

#[test]
fn changing_the_red_weight_changes_red_pixels_only() {
    let mut weights = DEFAULT_WEIGHTS;
    weights[0] = 100;
    let red = run8([255, 0, 0], *b"blwh", blwh(weights, None));
    assert!(close(red, [255, 255, 255], 1), "{red:?}");
    for pixel in [[0, 255, 0], [0, 0, 255], [0, 255, 255], [128, 128, 128]] {
        assert_eq!(
            run8(pixel, *b"blwh", blwh(weights, None)),
            run8(pixel, *b"blwh", blwh(DEFAULT_WEIGHTS, None)),
            "{pixel:?}"
        );
    }
}

#[test]
fn a_tint_makes_the_grey_colored() {
    let plain = run8([128, 128, 128], *b"blwh", blwh(DEFAULT_WEIGHTS, None));
    assert_eq!(spread(plain), 0);
    let tinted = run8(
        [128, 128, 128],
        *b"blwh",
        blwh(DEFAULT_WEIGHTS, Some([225.0, 211.0, 179.0])),
    );
    assert!(tinted[0] > tinted[1] && tinted[1] > tinted[2], "{tinted:?}");
}

#[test]
fn a_black_to_white_gradient_map_is_near_identity_on_grey() {
    let block = grdm(false, &black_to_white());
    for level in [0u8, 1, 64, 128, 200, 255] {
        let out = run8([level; 3], *b"grdm", block.clone());
        assert!(close(out, [level; 3], 1), "{level} -> {out:?}");
    }
}

#[test]
fn reversing_a_gradient_map_flips_the_ramp() {
    let block = grdm(true, &black_to_white());
    for level in [0u8, 64, 128, 255] {
        let out = run8([level; 3], *b"grdm", block.clone());
        assert!(close(out, [255 - level; 3], 1), "{level} -> {out:?}");
    }
}

#[test]
fn a_two_color_map_gives_the_start_color_on_black_and_the_end_on_white() {
    let block = grdm(false, &[(0, 50, [200, 30, 40]), (4096, 50, [20, 60, 220])]);
    assert!(close(
        run8([0; 3], *b"grdm", block.clone()),
        [200, 30, 40],
        1
    ));
    assert!(close(
        run8([255; 3], *b"grdm", block.clone()),
        [20, 60, 220],
        1
    ));
}

#[test]
fn a_gradient_map_midpoint_moves_the_middle_value() {
    let at = |midpoint: u32| {
        let block = grdm(false, &[(0, 50, [0; 3]), (4096, midpoint, [255; 3])]);
        run8([128; 3], *b"grdm", block)[0]
    };
    let neutral = at(50);
    assert!(close([neutral; 3], [128; 3], 1), "{neutral}");
    // The end stop's midpoint sets where the segment before it is halfway.
    assert!(at(25) > neutral + 20, "{}", at(25));
    assert!(at(75) + 20 < neutral, "{}", at(75));
}

#[test]
fn every_tone_block_agrees_between_eight_and_sixteen_bit() {
    let mut weights = DEFAULT_WEIGHTS;
    weights[1] = 90;
    let blocks: [(&[u8; 4], Vec<u8>); 8] = [
        (b"grdm", grdm_v3(&black_to_white())),
        (b"vibA", viba(Some(60), Some(25))),
        (b"vibA", viba(Some(-40), Some(-30))),
        (b"blwh", blwh(weights, None)),
        (b"blwh", blwh(weights, Some([225.0, 211.0, 179.0]))),
        (b"grdm", grdm(false, &black_to_white())),
        (
            b"grdm",
            grdm(true, &[(0, 30, [200, 30, 40]), (4096, 50, [20, 60, 220])]),
        ),
        (
            b"grdm",
            grdm(
                false,
                &[
                    (0, 50, [10, 10, 10]),
                    (2048, 50, [250, 200, 0]),
                    (4096, 50, [255; 3]),
                ],
            ),
        ),
    ];
    for pixel in [[180u8, 90, 60], [30, 160, 210], [120, 120, 130]] {
        for (key, block) in &blocks {
            let eight = run(8, pixel, **key, block.clone());
            let sixteen = run(16, pixel, **key, block.clone());
            assert!(
                close(eight, sixteen, 1),
                "{key:?} {pixel:?}: {eight:?} vs {sixteen:?}"
            );
        }
    }
}
