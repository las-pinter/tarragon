//! Fixed-layout adjustment blocks: `expA`, `mixr`, `blnc`, `hue2`, `selc`, `phfl`.
//!
//! Every read is bounds-checked. A short or bad block gives `None`, and the
//! layer then stays unsupported.

use crate::adjustment_color::{HueMaster, HueRange, lab_to_xyz, srgb_to_xyz};
use crate::adjustment_parse::{be16, be32, bef32, bei16, bei32};
use crate::adjustments::Adjustment;

/// The least size of a `hue2` block: 16 header bytes and six 14-byte ranges.
const HUE_BLOCK_LEN: usize = 100;

/// Parse one of the six fixed-layout color blocks.
#[must_use]
pub(crate) fn parse_color_adjustment(key: [u8; 4], data: &[u8]) -> Option<Adjustment> {
    match &key {
        b"expA" => parse_exposure(data),
        b"mixr" => parse_channel_mixer(data),
        b"blnc" => parse_color_balance(data),
        b"hue2" => parse_hue_saturation(data),
        b"selc" => parse_selective_color(data),
        b"phfl" => parse_photo_filter(data),
        _ => None,
    }
}

/// Read an `i16` as a float.
fn float16(data: &[u8], at: usize) -> Option<f32> {
    bei16(data, at).map(f32::from)
}

/// `expA`: version 1, then exposure, offset and gamma as `f32`.
fn parse_exposure(data: &[u8]) -> Option<Adjustment> {
    if be16(data, 0)? != 1 {
        return None;
    }
    let exposure = bef32(data, 2)?;
    let offset = bef32(data, 6)?;
    let gamma = bef32(data, 10)?;
    if ![exposure, offset, gamma]
        .iter()
        .all(|value| value.is_finite())
    {
        return None;
    }
    Some(Adjustment::Exposure {
        exposure,
        offset,
        gamma,
    })
}

/// `mixr`: version 1, a monochrome flag, then one 10-byte record per output
/// channel: red, green, blue, a fourth source for CMYK, and a constant. All
/// are percent. A monochrome block can hold only the first record.
fn parse_channel_mixer(data: &[u8]) -> Option<Adjustment> {
    if be16(data, 0)? != 1 {
        return None;
    }
    let monochrome = be16(data, 2)? != 0;
    let record = |index: usize| -> Option<[f32; 4]> {
        let at = 4 + index * 10;
        let value = |slot: usize| {
            float16(data, at + slot * 2).map(|percent| percent.clamp(-200.0, 200.0) / 100.0)
        };
        Some([value(0)?, value(1)?, value(2)?, value(4)?])
    };
    let first = record(0)?;
    let (second, third) = if monochrome {
        (record(1).unwrap_or([0.0; 4]), record(2).unwrap_or([0.0; 4]))
    } else {
        (record(1)?, record(2)?)
    };
    Some(Adjustment::ChannelMixer {
        matrix: [first, second, third],
        monochrome,
    })
}

/// `blnc`: nine `i16` (shadows, midtones, highlights; each cyan-red,
/// magenta-green, yellow-blue), then a preserve-luminosity byte.
fn parse_color_balance(data: &[u8]) -> Option<Adjustment> {
    let mut tones = [[0.0f32; 3]; 3];
    for (tone, row) in tones.iter_mut().enumerate() {
        for (axis, value) in row.iter_mut().enumerate() {
            *value = float16(data, (tone * 3 + axis) * 2)?.clamp(-100.0, 100.0);
        }
    }
    Some(Adjustment::ColorBalance {
        tones,
        preserve_luminosity: *data.get(18)? != 0,
    })
}

/// `hue2`: version 2, a colorize byte, a pad byte, the colorize values (hue,
/// saturation, lightness) at byte 4, the master values at byte 10, then six
/// ranges of seven `i16` from byte 16: four hue bounds, then hue, saturation
/// and lightness.
///
/// Photoshop always writes the full 100 bytes. A shorter block gives `None`.
fn parse_hue_saturation(data: &[u8]) -> Option<Adjustment> {
    if data.len() < HUE_BLOCK_LEN || be16(data, 0)? != 2 {
        return None;
    }
    let colorize = *data.get(2)? != 0;
    let base = if colorize { 4 } else { 10 };
    let master = HueMaster {
        hue: float16(data, base)?,
        saturation: float16(data, base + 2)?,
        lightness: float16(data, base + 4)?,
        colorize,
    };
    let mut ranges = HueRange::defaults();
    for (index, range) in ranges.iter_mut().enumerate() {
        let at = 16 + index * 14;
        let value = |slot: usize| float16(data, at + slot * 2);
        *range = HueRange {
            hue: value(4)?,
            saturation: value(5)?,
            lightness: value(6)?,
            bounds: HueRange::canonical_bounds([value(0)?, value(1)?, value(2)?, value(3)?]),
        };
    }
    Some(Adjustment::HueSaturation { master, ranges })
}

/// `selc`: version 1, a method (0 relative, 1 absolute), 8 reserved bytes,
/// then nine records of four `i16` (cyan, magenta, yellow, black).
fn parse_selective_color(data: &[u8]) -> Option<Adjustment> {
    if be16(data, 0)? != 1 {
        return None;
    }
    let relative = be16(data, 2)? == 0;
    let mut records = [[0.0f32; 4]; 9];
    for (index, record) in records.iter_mut().enumerate() {
        for (slot, value) in record.iter_mut().enumerate() {
            *value = float16(data, 12 + index * 8 + slot * 2)?;
        }
    }
    Some(Adjustment::SelectiveColor { relative, records })
}

/// The D50 XYZ filter color of a version 3 `phfl` block.
///
/// UNVERIFIED: the raw values are read as XYZ relative to the D50 white,
/// where 32768 is 1.0. This matches the Photoshop preset colors in one public
/// description. Another description reads them as CIE Lab times 100. No
/// Photoshop-made version 3 file in this repository settles the question. To
/// switch, change this function only.
#[allow(
    clippy::cast_precision_loss,
    reason = "a 32-bit sample fits a float closely enough for a color"
)]
fn photo_filter_v3_xyz(raw: [i32; 3]) -> Option<[f32; 3]> {
    let xyz = raw.map(|value| value as f32 / 32768.0);
    xyz.iter()
        .all(|value| (0.0..=2.0).contains(value))
        .then_some(xyz)
}

/// The D50 XYZ filter color of a version 2 `phfl` color: a color space and
/// four `u16`. RGB, CMYK, Lab and grayscale are known.
fn photo_filter_v2_xyz(space: u16, words: [u16; 4]) -> Option<[f32; 3]> {
    let unit = |word: u16| f32::from(word) / 65535.0;
    match space {
        0 => Some(srgb_to_xyz([
            unit(words[0]),
            unit(words[1]),
            unit(words[2]),
        ])),
        // CMYK words are inverted: 65535 means no ink.
        2 => {
            let ink = words.map(|word| 1.0 - unit(word));
            let rgb = [0, 1, 2].map(|index| (1.0 - ink[index]) * (1.0 - ink[3]));
            Some(srgb_to_xyz(rgb))
        }
        7 => {
            #[allow(
                clippy::cast_possible_wrap,
                reason = "the two's-complement bit pattern is the signed value"
            )]
            let signed = |word: u16| f32::from(word as i16) / 100.0;
            Some(lab_to_xyz([
                f32::from(words[0].min(10_000)) / 100.0,
                signed(words[1]),
                signed(words[2]),
            ]))
        }
        8 => {
            let grey = 1.0 - f32::from(words[0].min(10_000)) / 10_000.0;
            Some(srgb_to_xyz([grey; 3]))
        }
        _ => None,
    }
}

/// `phfl`: version 2 or 3, a color, the density in percent as `u32`, and a
/// preserve-luminosity byte. Version 2 has a color space and four `u16`.
/// Version 3 has three `i32`.
fn parse_photo_filter(data: &[u8]) -> Option<Adjustment> {
    let (filter_xyz, at) = match be16(data, 0)? {
        2 => {
            let words = [
                be16(data, 4)?,
                be16(data, 6)?,
                be16(data, 8)?,
                be16(data, 10)?,
            ];
            (photo_filter_v2_xyz(be16(data, 2)?, words)?, 12)
        }
        3 => {
            let raw = [bei32(data, 2)?, bei32(data, 6)?, bei32(data, 10)?];
            (photo_filter_v3_xyz(raw)?, 14)
        }
        _ => return None,
    };
    #[allow(clippy::cast_precision_loss, reason = "a percent is a small integer")]
    let density = (be32(data, at)? as f32 / 100.0).clamp(0.0, 1.0);
    Some(Adjustment::PhotoFilter {
        filter_xyz,
        density,
        preserve_luminosity: *data.get(at + 4)? != 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adjustments::parse_adjustment;

    fn words(values: &[i16]) -> Vec<u8> {
        values
            .iter()
            .flat_map(|value| value.to_be_bytes())
            .collect()
    }

    #[test]
    fn an_exposure_block_parses_three_floats() {
        let mut data = 1u16.to_be_bytes().to_vec();
        for value in [-1.0f32, 0.25, 1.5] {
            data.extend_from_slice(&value.to_be_bytes());
        }
        assert_eq!(
            parse_adjustment(b"expA", &data, 256),
            Some(Adjustment::Exposure {
                exposure: -1.0,
                offset: 0.25,
                gamma: 1.5
            })
        );
        assert!(parse_adjustment(b"expA", &data[..13], 256).is_none());
    }

    #[test]
    fn a_non_finite_exposure_is_rejected() {
        let mut data = 1u16.to_be_bytes().to_vec();
        for value in [f32::NAN, 0.0, 1.0] {
            data.extend_from_slice(&value.to_be_bytes());
        }
        assert!(parse_adjustment(b"expA", &data, 256).is_none());
    }

    #[test]
    fn a_mixer_block_parses_its_records() {
        // Version 1, color mode, then red out: 50, 25, 0, 0, constant 10.
        let mut values = vec![1, 0, 50, 25, 0, 0, 10];
        // Green out: green 100. Blue out: blue 100.
        values.extend_from_slice(&[0, 100, 0, 0, 0, 0, 0, 100, 0, 0]);
        let Some(Adjustment::ChannelMixer { matrix, monochrome }) =
            parse_adjustment(b"mixr", &words(&values), 256)
        else {
            panic!("mixer");
        };
        assert!(!monochrome);
        assert_eq!(matrix[0], [0.5, 0.25, 0.0, 0.1]);
        assert_eq!(matrix[1], [0.0, 1.0, 0.0, 0.0]);
        assert_eq!(matrix[2], [0.0, 0.0, 1.0, 0.0]);
    }

    #[test]
    fn a_monochrome_mixer_needs_only_the_first_record() {
        let data = words(&[1, 1, 0, 100, 0, 0, 0]);
        assert!(matches!(
            parse_adjustment(b"mixr", &data, 256),
            Some(Adjustment::ChannelMixer {
                monochrome: true,
                ..
            })
        ));
        // A color-mode block with a missing record is short.
        let data = words(&[1, 0, 0, 100, 0, 0, 0]);
        assert!(parse_adjustment(b"mixr", &data, 256).is_none());
    }

    #[test]
    fn a_balance_block_parses_nine_values_and_a_flag() {
        let mut data = words(&[10, -20, 30, 0, 0, 0, -100, 100, 150]);
        data.push(1);
        let Some(Adjustment::ColorBalance {
            tones,
            preserve_luminosity,
        }) = parse_adjustment(b"blnc", &data, 256)
        else {
            panic!("balance");
        };
        assert!(preserve_luminosity);
        assert_eq!(tones[0], [10.0, -20.0, 30.0]);
        assert_eq!(tones[2], [-100.0, 100.0, 100.0]);
        assert!(parse_adjustment(b"blnc", &data[..18], 256).is_none());
    }

    #[test]
    fn a_hue_block_parses_master_and_ranges() {
        let mut values = vec![2, 0x0000, 0, 0, 0, 40, -10, 5];
        // The reds range: bounds 315, 345, 15, 45, then hue 20, saturation 30, lightness -5.
        values.extend_from_slice(&[315, 345, 15, 45, 20, 30, -5]);
        values.resize(50, 0);
        let Some(Adjustment::HueSaturation { master, ranges }) =
            parse_adjustment(b"hue2", &words(&values), 256)
        else {
            panic!("hue");
        };
        assert!(!master.colorize);
        assert_eq!(
            (master.hue, master.saturation, master.lightness),
            (40.0, -10.0, 5.0)
        );
        assert_eq!(ranges[0].bounds, [-45.0, -15.0, 15.0, 45.0]);
        assert_eq!(ranges[0].hue, 20.0);
        assert!(ranges[1].is_neutral());
        // A block under 100 bytes is short.
        let short = words(&values[..49]);
        assert!(parse_adjustment(b"hue2", &short, 256).is_none());
    }

    #[test]
    fn a_colorize_block_reads_the_colorize_values() {
        let mut data = words(&[2, 0x0100, 200, 50, -10, 7, 8, 9]);
        data.resize(100, 0);
        let Some(Adjustment::HueSaturation { master, .. }) = parse_adjustment(b"hue2", &data, 256)
        else {
            panic!("hue");
        };
        assert!(master.colorize);
        assert_eq!(
            (master.hue, master.saturation, master.lightness),
            (200.0, 50.0, -10.0)
        );
        assert!(parse_adjustment(b"hue2", &data[..99], 256).is_none());
    }

    #[test]
    fn a_selective_block_parses_nine_records() {
        let mut values = vec![1, 1, 0, 0, 0, 0];
        values.extend_from_slice(&[10, 20, 30, 40]);
        values.extend_from_slice(&[0; 32]);
        values.extend_from_slice(&[0; 4]);
        let mut data = words(&values);
        data.resize(12 + 72, 0);
        let Some(Adjustment::SelectiveColor { relative, records }) =
            parse_adjustment(b"selc", &data, 256)
        else {
            panic!("selective");
        };
        assert!(!relative);
        assert_eq!(records[0], [10.0, 20.0, 30.0, 40.0]);
        assert!(parse_adjustment(b"selc", &data[..83], 256).is_none());
    }

    #[test]
    fn a_photo_filter_block_parses_both_versions() {
        // Version 2, RGB, density 25 percent, preserve on.
        let mut v2 = words(&[2, 0]);
        v2.extend_from_slice(&[0xff, 0xff, 0x80, 0x00, 0, 0, 0, 0]);
        v2.extend_from_slice(&25u32.to_be_bytes());
        v2.push(1);
        let Some(Adjustment::PhotoFilter {
            density,
            preserve_luminosity,
            ..
        }) = parse_adjustment(b"phfl", &v2, 256)
        else {
            panic!("photo filter");
        };
        assert!((density - 0.25).abs() < 1e-6 && preserve_luminosity);
        assert!(parse_adjustment(b"phfl", &v2[..v2.len() - 1], 256).is_none());

        // Version 3, XYZ equal to the white point times 32768.
        let mut v3 = 3u16.to_be_bytes().to_vec();
        for value in [31_595i32, 32_768, 27_042] {
            v3.extend_from_slice(&value.to_be_bytes());
        }
        v3.extend_from_slice(&100u32.to_be_bytes());
        v3.push(0);
        let Some(Adjustment::PhotoFilter { filter_xyz, .. }) = parse_adjustment(b"phfl", &v3, 256)
        else {
            panic!("photo filter v3");
        };
        assert!((filter_xyz[1] - 1.0).abs() < 1e-6);
        assert!(parse_adjustment(b"phfl", &words(&[4]), 256).is_none());
    }

    #[test]
    fn a_photo_filter_with_an_unknown_space_is_rejected() {
        let mut data = words(&[2, 5, 0, 0, 0, 0]);
        data.extend_from_slice(&[0, 0, 0, 25, 1]);
        assert!(parse_adjustment(b"phfl", &data, 256).is_none());
    }

    #[test]
    fn empty_color_blocks_never_panic() {
        for key in [b"expA", b"mixr", b"blnc", b"hue2", b"selc", b"phfl"] {
            assert!(parse_adjustment(key, &[], 256).is_none());
            assert!(parse_adjustment(key, &[0xff; 3], 256).is_none());
        }
    }
}
