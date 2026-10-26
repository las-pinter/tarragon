//! Parsers for `vibA`, `blwh` (versioned descriptors) and `grdm` (fixed binary).
//!
//! Every read is bounds-checked. A short or bad block gives `None`, and the
//! layer then stays unsupported.

use crate::adjustment_parse::{be16, be32};
use crate::adjustments::Adjustment;
use crate::content::{color_from_descriptor, sort_stops};
use crate::descriptor::{Descriptor, parse_versioned};

/// The Black & White keys: reds, yellows, greens, cyans, blues, magentas.
const BW_KEYS: [&[u8]; 6] = [b"Rd  ", b"Yllw", b"Grn ", b"Cyn ", b"Bl  ", b"Mgnt"];

/// The default Black & White weights, in percent.
const BW_DEFAULTS: [f32; 6] = [40.0, 60.0, 40.0, 60.0, 20.0, 80.0];

/// The tint used when `useTint` is set but the color is missing.
const BW_DEFAULT_TINT: [f32; 3] = [225.0 / 255.0, 211.0 / 255.0, 179.0 / 255.0];

/// The size of one color stop in a `grdm` block.
const COLOR_STOP_LEN: usize = 20;

/// The size of one transparency stop in a `grdm` block.
const OPACITY_STOP_LEN: usize = 10;

/// Parse one of `vibA`, `blwh` or `grdm`.
#[must_use]
pub(crate) fn parse_tone_adjustment(key: [u8; 4], data: &[u8]) -> Option<Adjustment> {
    match &key {
        b"vibA" => parse_vibrance(data),
        b"blwh" => parse_black_white(data),
        b"grdm" => parse_gradient_map(data),
        _ => None,
    }
}

/// `vibA`: a versioned descriptor with `vibrance` and `Strt` (saturation).
/// A missing key means 0.
#[allow(
    clippy::cast_possible_truncation,
    reason = "the sliders are small integers"
)]
fn parse_vibrance(data: &[u8]) -> Option<Adjustment> {
    let (descriptor, _) = parse_versioned(data)?;
    let slider = |key: &[u8]| {
        descriptor
            .number(key)
            .filter(|value| value.is_finite())
            .unwrap_or(0.0)
            .clamp(-100.0, 100.0) as f32
    };
    Some(Adjustment::Vibrance {
        vibrance: slider(b"vibrance"),
        saturation: slider(b"Strt"),
    })
}

/// `blwh`: a versioned descriptor with six weights in percent, `useTint` and
/// a `tintColor` color descriptor.
#[allow(
    clippy::cast_possible_truncation,
    reason = "the weights are small percentages"
)]
fn parse_black_white(data: &[u8]) -> Option<Adjustment> {
    let (descriptor, _) = parse_versioned(data)?;
    let mut weights = BW_DEFAULTS;
    for (weight, key) in weights.iter_mut().zip(BW_KEYS) {
        if let Some(value) = descriptor.number(key).filter(|value| value.is_finite()) {
            *weight = value.clamp(-200.0, 300.0) as f32;
        }
    }
    let tint = descriptor
        .boolean(b"useTint")
        .unwrap_or(false)
        .then(|| tint_color(&descriptor));
    Some(Adjustment::BlackWhite { weights, tint })
}

/// The tint color of a Black & White descriptor, clamped to `0.0..=1.0`.
fn tint_color(descriptor: &Descriptor) -> [f32; 3] {
    descriptor
        .descriptor(b"tintColor")
        .and_then(color_from_descriptor)
        .map_or(BW_DEFAULT_TINT, |color| {
            color.map(|value| {
                if value.is_finite() {
                    value.clamp(0.0, 1.0)
                } else {
                    0.0
                }
            })
        })
}

/// `grdm`: a `u16` version (1, or 3 with a 4-byte interpolation method), a
/// reverse byte, a dither byte, a Unicode name, then the color stops and the
/// transparency stops.
///
/// A color stop has 20 bytes: location over 4096, midpoint percent, color
/// space, four `u16` components and two pad bytes. Only RGB stops are read.
/// The transparency stops, the smoothness, the interpolation method and the
/// noise-gradient fields do not change the color, so they are skipped. A
/// block with fewer than two color stops gives `None`.
fn parse_gradient_map(data: &[u8]) -> Option<Adjustment> {
    let mut at = match be16(data, 0)? {
        1 => 4,
        3 => 8,
        _ => return None,
    };
    let reverse = *data.get(2)? != 0;
    // The dither byte must exist, but dither is not applied.
    let _dither = data.get(3)?;
    if at == 8 {
        data.get(4..8)?;
    }
    // The name: a unit count, then two bytes per unit.
    let name_len = usize::try_from(be32(data, at)?).ok()?.checked_mul(2)?;
    at = at.checked_add(4)?.checked_add(name_len)?;
    let count = usize::from(be16(data, at)?);
    at += 2;
    if count < 2 || at.checked_add(count.checked_mul(COLOR_STOP_LEN)?)? > data.len() {
        return None;
    }
    let mut stops = Vec::with_capacity(count);
    let mut midpoints = Vec::with_capacity(count);
    for _ in 0..count {
        if be16(data, at + 8)? != 0 {
            return None;
        }
        #[allow(
            clippy::cast_precision_loss,
            reason = "the location and midpoint are small integers"
        )]
        let (location, midpoint) = (
            (be32(data, at)? as f32 / 4096.0).clamp(0.0, 1.0),
            (be32(data, at + 4)? as f32 / 100.0).clamp(0.0, 1.0),
        );
        let channel = |slot: usize| be16(data, at + 10 + slot * 2).map(|v| f32::from(v) / 65535.0);
        stops.push((location, [channel(0)?, channel(1)?, channel(2)?, 1.0]));
        midpoints.push(midpoint);
        at += COLOR_STOP_LEN;
    }
    // The transparency stops must be present and whole, then the expansion count.
    let opacity_count = usize::from(be16(data, at)?);
    let end = at
        .checked_add(2)?
        .checked_add(opacity_count.checked_mul(OPACITY_STOP_LEN)?)?
        .checked_add(2)?;
    if end > data.len() {
        return None;
    }
    let (stops, midpoints) = sort_stops(&stops, &midpoints);
    Some(Adjustment::GradientMap {
        stops,
        midpoints,
        reverse,
    })
}

#[cfg(test)]
mod tests {
    use super::{Adjustment, parse_tone_adjustment};
    use crate::adjustment_parse::parse_adjustment;

    /// Build a `grdm` block with the given stops: location, midpoint, rgb.
    fn grdm(version: u16, reverse: bool, stops: &[(u32, u32, [u16; 3])]) -> Vec<u8> {
        let mut data = version.to_be_bytes().to_vec();
        data.extend_from_slice(&[u8::from(reverse), 0]);
        if version == 3 {
            data.extend_from_slice(b"Lnr ");
        }
        data.extend_from_slice(&2u32.to_be_bytes());
        data.extend_from_slice(&[0, b'a', 0, 0]);
        data.extend_from_slice(&u16::try_from(stops.len()).expect("count").to_be_bytes());
        for (location, midpoint, rgb) in stops {
            data.extend_from_slice(&location.to_be_bytes());
            data.extend_from_slice(&midpoint.to_be_bytes());
            data.extend_from_slice(&0u16.to_be_bytes());
            for value in rgb {
                data.extend_from_slice(&value.to_be_bytes());
            }
            data.extend_from_slice(&[0; 4]);
        }
        // One transparency stop, then the expansion count.
        data.extend_from_slice(&1u16.to_be_bytes());
        data.extend_from_slice(&[0; 10]);
        data.extend_from_slice(&2u16.to_be_bytes());
        data
    }

    fn two_stops() -> [(u32, u32, [u16; 3]); 2] {
        [(0, 50, [0, 0, 0]), (4096, 50, [65535, 32768, 0])]
    }

    #[test]
    fn a_gradient_map_parses_in_both_versions() {
        for version in [1, 3] {
            let data = grdm(version, true, &two_stops());
            let Some(Adjustment::GradientMap {
                stops,
                midpoints,
                reverse,
            }) = parse_tone_adjustment(*b"grdm", &data)
            else {
                panic!("grdm v{version}");
            };
            assert!(reverse);
            assert_eq!(stops.len(), 2);
            assert!((stops[1].0 - 1.0).abs() < 1e-6);
            assert!((stops[1].1[1] - 32768.0 / 65535.0).abs() < 1e-6);
            assert_eq!(midpoints, vec![0.5]);
        }
    }

    #[test]
    fn gradient_map_stops_are_sorted_and_keep_their_midpoints() {
        let stops = [(4096, 30, [65535; 3]), (0, 50, [0; 3])];
        let data = grdm(1, false, &stops);
        let Some(Adjustment::GradientMap {
            stops, midpoints, ..
        }) = parse_tone_adjustment(*b"grdm", &data)
        else {
            panic!("grdm");
        };
        assert!(stops[0].0 < stops[1].0);
        assert_eq!(midpoints, vec![0.3]);
    }

    #[test]
    fn a_short_or_bad_gradient_map_gives_none() {
        let data = grdm(1, false, &two_stops());
        for len in 0..data.len() {
            assert!(
                parse_tone_adjustment(*b"grdm", &data[..len]).is_none(),
                "len {len}"
            );
        }
        assert!(parse_tone_adjustment(*b"grdm", &data).is_some());
        assert!(parse_tone_adjustment(*b"grdm", &grdm(2, false, &two_stops())).is_none());
        assert!(parse_tone_adjustment(*b"grdm", &grdm(1, false, &two_stops()[..1])).is_none());
        // A color space other than RGB is not read.
        let mut hsb = data.clone();
        hsb[4 + 4 + 4 + 2 + 8 + 1] = 1;
        assert!(parse_tone_adjustment(*b"grdm", &hsb).is_none());
        // A name length that points far past the data must not panic.
        let mut huge = data;
        huge[4..8].copy_from_slice(&u32::MAX.to_be_bytes());
        assert!(parse_tone_adjustment(*b"grdm", &huge).is_none());
    }

    /// Write a descriptor id: zero for a four-byte id, else a length.
    fn push_id(data: &mut Vec<u8>, id: &[u8]) {
        let length = if id.len() == 4 { 0 } else { id.len() };
        data.extend_from_slice(&u32::try_from(length).expect("id").to_be_bytes());
        data.extend_from_slice(id);
    }

    /// A versioned descriptor body: a class id and items of a key and a typed value.
    fn body(class: &[u8], items: &[(&[u8], Vec<u8>)]) -> Vec<u8> {
        let mut data = 0u32.to_be_bytes().to_vec();
        push_id(&mut data, class);
        data.extend_from_slice(&u32::try_from(items.len()).expect("count").to_be_bytes());
        for (key, value) in items {
            push_id(&mut data, key);
            data.extend_from_slice(value);
        }
        data
    }

    fn versioned(body: &[u8]) -> Vec<u8> {
        let mut data = 16u32.to_be_bytes().to_vec();
        data.extend_from_slice(body);
        data
    }

    fn long(value: i32) -> Vec<u8> {
        let mut data = b"long".to_vec();
        data.extend_from_slice(&value.to_be_bytes());
        data
    }

    fn boolean(value: bool) -> Vec<u8> {
        let mut data = b"bool".to_vec();
        data.push(u8::from(value));
        data
    }

    fn rgb_color(red: f64, green: f64, blue: f64) -> Vec<u8> {
        let double = |value: f64| {
            let mut data = b"doub".to_vec();
            data.extend_from_slice(&value.to_be_bytes());
            data
        };
        let mut data = b"Objc".to_vec();
        data.extend_from_slice(&body(
            b"RGBC",
            &[
                (b"Rd  ", double(red)),
                (b"Grn ", double(green)),
                (b"Bl  ", double(blue)),
            ],
        ));
        data
    }

    #[test]
    fn a_vibrance_block_reads_its_sliders_and_defaults_missing_keys_to_zero() {
        let data = versioned(&body(
            b"null",
            &[(b"vibrance", long(30)), (b"Strt", long(-500))],
        ));
        assert_eq!(
            parse_tone_adjustment(*b"vibA", &data),
            Some(Adjustment::Vibrance {
                vibrance: 30.0,
                saturation: -100.0
            })
        );
        let empty = versioned(&body(b"null", &[]));
        assert_eq!(
            parse_tone_adjustment(*b"vibA", &empty),
            Some(Adjustment::Vibrance {
                vibrance: 0.0,
                saturation: 0.0
            })
        );
        for len in 0..data.len() {
            assert!(parse_tone_adjustment(*b"vibA", &data[..len]).is_none());
        }
    }

    #[test]
    fn a_black_white_block_reads_weights_and_the_tint() {
        let defaults = versioned(&body(b"null", &[]));
        assert_eq!(
            parse_tone_adjustment(*b"blwh", &defaults),
            Some(Adjustment::BlackWhite {
                weights: [40.0, 60.0, 40.0, 60.0, 20.0, 80.0],
                tint: None
            })
        );
        let data = versioned(&body(
            b"null",
            &[
                (b"Rd  ", long(90)),
                (b"Mgnt", long(-10)),
                (b"useTint", boolean(true)),
                (b"tintColor", rgb_color(255.0, 0.0, 51.0)),
            ],
        ));
        let Some(Adjustment::BlackWhite { weights, tint }) = parse_tone_adjustment(*b"blwh", &data)
        else {
            panic!("blwh");
        };
        assert_eq!(weights, [90.0, 60.0, 40.0, 60.0, 20.0, -10.0]);
        let tint = tint.expect("tint");
        assert!((tint[0] - 1.0).abs() < 1e-6 && tint[1].abs() < 1e-6);
        assert!((tint[2] - 0.2).abs() < 1e-6);
        for len in 0..data.len() {
            assert!(parse_tone_adjustment(*b"blwh", &data[..len]).is_none());
        }
        // A tint switched on without a color falls back to the default tint.
        let no_color = versioned(&body(b"null", &[(b"useTint", boolean(true))]));
        assert!(matches!(
            parse_tone_adjustment(*b"blwh", &no_color),
            Some(Adjustment::BlackWhite { tint: Some(_), .. })
        ));
    }

    #[test]
    fn descriptor_blocks_with_a_bad_version_give_none() {
        assert!(parse_adjustment(b"vibA", &[0, 0, 0, 15, 0], 256).is_none());
        assert!(parse_adjustment(b"blwh", &[], 256).is_none());
        assert!(parse_adjustment(b"vibA", &[0, 0, 0], 256).is_none());
    }
}
