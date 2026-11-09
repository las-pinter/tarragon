//! Adjustment block builders shared by the PSD test binaries.
//!
//! The helpers build the `vibA`, `blwh` and `grdm` blocks and a Color Lookup
//! cube text. Several test binaries need the same bytes, and one binary cannot
//! import from another.

use std::fmt::Write;

use super::encoders::{
    descriptor_body, descriptor_item, rgbc, value_boolean, value_descriptor, value_integer,
    versioned,
};

/// Encode each key and value pair as one descriptor item.
pub fn items(entries: &[(&[u8], Vec<u8>)]) -> Vec<Vec<u8>> {
    entries
        .iter()
        .map(|(key, value)| descriptor_item(key, value))
        .collect()
}

/// A `vibA` block. `None` leaves the key out, as Photoshop does for zero.
pub fn viba(vibrance: Option<i32>, saturation: Option<i32>) -> Vec<u8> {
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
pub fn blwh(weights: [i32; 6], tint: Option<[f64; 3]>) -> Vec<u8> {
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

/// The Photoshop default `blwh` weights.
pub const DEFAULT_WEIGHTS: [i32; 6] = [40, 60, 40, 60, 20, 80];

/// A `grdm` block with the full gradient record. Each stop is a location out
/// of 4096, a midpoint percent and an 8-bit color.
pub fn grdm(reverse: bool, stops: &[(u32, u32, [u8; 3])]) -> Vec<u8> {
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
    // Two transparency stops.
    data.extend_from_slice(&2u16.to_be_bytes());
    for location in [0u32, 4096] {
        data.extend_from_slice(&location.to_be_bytes());
        data.extend_from_slice(&50u32.to_be_bytes());
        data.extend_from_slice(&0x7fffu16.to_be_bytes());
    }
    // The expansion count, the smoothness, the stop length and the mode.
    for value in [2u16, 4096, 32, 0] {
        data.extend_from_slice(&value.to_be_bytes());
    }
    // The noise-gradient tail with neutral values: random seed, show
    // transparency, use vector color, roughness, color model, minimum and
    // maximum color, and two pad bytes. Photoshop writes the tail for every
    // gradient, and other readers fail on a block that ends before it.
    data.extend_from_slice(&0u32.to_be_bytes());
    data.extend_from_slice(&[0; 4]);
    data.extend_from_slice(&0u32.to_be_bytes());
    data.extend_from_slice(&0u16.to_be_bytes());
    data.extend_from_slice(&[0; 16]);
    data.extend_from_slice(&[0; 2]);
    data
}

/// The two stops of a black-to-white gradient, for `grdm`.
pub fn black_to_white() -> Vec<(u32, u32, [u8; 3])> {
    vec![(0, 50, [0; 3]), (4096, 50, [255; 3])]
}

/// A 2x2x2 cube text; `map` turns the (r, g, b) corner into its output.
pub fn cube(map: impl Fn([f32; 3]) -> [f32; 3]) -> Vec<u8> {
    let mut text = String::from("TITLE \"t\"\nLUT_3D_SIZE 2\n");
    for b in [0.0f32, 1.0] {
        for g in [0.0f32, 1.0] {
            for r in [0.0f32, 1.0] {
                let out = map([r, g, b]);
                let _ = writeln!(text, "{} {} {}", out[0], out[1], out[2]);
            }
        }
    }
    text.into_bytes()
}

/// Hue, saturation and lightness edits for six color ranges, all zero.
pub const NO_EDITS: [[i16; 3]; 6] = [[0; 3]; 6];
