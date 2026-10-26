//! Descriptor block parsing for adjustment layers.
//!
//! This module reads the Adobe PSD binary blocks and builds the adjustment
//! value types from [`crate::adjustments`]. Keys that are unknown or blocks
//! that are malformed give `None`.

use crate::adjustment_parse_color::parse_color_adjustment;
use crate::adjustments::{Adjustment, Curve, CurvePoint, LevelsChannel};
use crate::descriptor::parse_versioned;

/// Read a big-endian `u16`.
#[must_use]
pub(crate) fn be16(data: &[u8], at: usize) -> Option<u16> {
    data.get(at..at + 2)
        .map(|bytes| u16::from_be_bytes([bytes[0], bytes[1]]))
}

/// Read a big-endian `i16`.
#[allow(
    clippy::cast_possible_wrap,
    reason = "the two's-complement bit pattern is the signed value"
)]
#[must_use]
pub(crate) fn bei16(data: &[u8], at: usize) -> Option<i16> {
    be16(data, at).map(|value| value as i16)
}

/// Read a big-endian `u32`.
#[must_use]
pub(crate) fn be32(data: &[u8], at: usize) -> Option<u32> {
    data.get(at..at.checked_add(4)?)
        .map(|bytes| u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

/// Read a big-endian `i32`.
#[allow(
    clippy::cast_possible_wrap,
    reason = "the two's-complement bit pattern is the signed value"
)]
#[must_use]
pub(crate) fn bei32(data: &[u8], at: usize) -> Option<i32> {
    be32(data, at).map(|value| value as i32)
}

/// Read a big-endian `f32`.
#[must_use]
pub(crate) fn bef32(data: &[u8], at: usize) -> Option<f32> {
    be32(data, at).map(f32::from_bits)
}

/// Parse one adjustment block other than brightness/contrast.
///
/// Returns `None` for an unknown key or a malformed block.
#[must_use]
pub fn parse_adjustment(key: &[u8; 4], data: &[u8], lut_size: u32) -> Option<Adjustment> {
    match key {
        b"nvrt" => Some(Adjustment::Invert),
        b"thrs" => be16(data, 0).map(|value| Adjustment::Threshold {
            level: f32::from(value) / 255.0,
        }),
        b"post" => be16(data, 0).map(|value| Adjustment::Posterize {
            // One level or none would divide by zero in the bin math.
            levels: u8::try_from(value.clamp(2, 255)).unwrap_or(255),
            lut_size,
        }),
        b"levl" => parse_levels(data),
        b"curv" => parse_curves(data),
        b"expA" | b"mixr" | b"blnc" | b"hue2" | b"selc" | b"phfl" => {
            parse_color_adjustment(*key, data)
        }
        _ => None,
    }
}

/// Parse a `brit` block (and its optional `CgEd` modern parameters).
#[allow(
    clippy::cast_possible_truncation,
    reason = "the descriptor holds small integer percentages"
)]
#[must_use]
pub fn parse_brightness_contrast(data: &[u8], cged: Option<&[u8]>) -> Option<Adjustment> {
    if let Some(descriptor) = cged.and_then(|block| parse_versioned(block).map(|(inner, _)| inner))
        && let (Some(brightness), Some(contrast)) =
            (descriptor.number(b"Brgh"), descriptor.number(b"Cntr"))
    {
        return Some(Adjustment::BrightnessContrast {
            brightness: brightness as f32,
            contrast: contrast as f32,
            legacy: descriptor.boolean(b"useLegacy").unwrap_or(false),
        });
    }
    let brightness = bei16(data, 0)?;
    let contrast = bei16(data, 2)?;
    Some(Adjustment::BrightnessContrast {
        brightness: f32::from(brightness),
        contrast: f32::from(contrast),
        legacy: true,
    })
}

/// Parse a `levl` block: a version, then five ten-byte records.
#[must_use]
fn parse_levels(data: &[u8]) -> Option<Adjustment> {
    if be16(data, 0)? != 2 {
        return None;
    }
    let master = levels_record(data, 2)?;
    let red = levels_record(data, 12)?;
    let green = levels_record(data, 22)?;
    let blue = levels_record(data, 32)?;
    Some(Adjustment::Levels {
        master,
        per_channel: [red, green, blue],
    })
}

/// Parse one ten-byte levels record.
#[must_use]
fn levels_record(data: &[u8], at: usize) -> Option<LevelsChannel> {
    Some(LevelsChannel {
        input_black: f32::from(be16(data, at)?) / 255.0,
        input_white: f32::from(be16(data, at + 2)?) / 255.0,
        output_black: f32::from(be16(data, at + 4)?) / 255.0,
        output_white: f32::from(be16(data, at + 6)?) / 255.0,
        gamma: f32::from(be16(data, at + 8)?) / 100.0,
    })
}

/// Parse a `curv` block: a header, then one point list per set channel.
#[must_use]
fn parse_curves(data: &[u8]) -> Option<Adjustment> {
    let version = be16(data, 1)?;
    if version != 1 && version != 4 {
        return None;
    }
    let bits = u32::from_be_bytes(data.get(3..7)?.try_into().ok()?);
    let mut at = 7;
    let mut curves: [Vec<CurvePoint>; 4] = std::array::from_fn(|_| Vec::new());
    for bit in 0..32usize {
        if bits & (1 << bit) == 0 {
            continue;
        }
        let count = usize::from(be16(data, at)?);
        at += 2;
        // Every point is four bytes; refuse a count that cannot fit.
        if at.checked_add(count.checked_mul(4)?)? > data.len() {
            return None;
        }
        let mut points = Vec::with_capacity(count);
        for _ in 0..count {
            let output = f32::from(be16(data, at)?) / 255.0;
            let input = f32::from(be16(data, at + 2)?) / 255.0;
            at += 4;
            points.push(CurvePoint { input, output });
        }
        if let Some(slot) = curves.get_mut(bit) {
            *slot = points;
        }
    }
    let mut channel = curves;
    let master = Curve::new(std::mem::take(&mut channel[0]));
    let per_channel = [
        Curve::new(std::mem::take(&mut channel[1])),
        Curve::new(std::mem::take(&mut channel[2])),
        Curve::new(std::mem::take(&mut channel[3])),
    ];
    Some(Adjustment::Curves {
        master,
        per_channel,
    })
}

#[cfg(test)]
mod tests {
    use super::{Adjustment, parse_adjustment, parse_brightness_contrast};

    #[test]
    fn a_threshold_block_parses_its_level() {
        let adjustment = parse_adjustment(b"thrs", &[0, 128, 0, 0], 256).expect("threshold");
        assert_eq!(
            adjustment,
            Adjustment::Threshold {
                level: 128.0 / 255.0
            }
        );
    }

    #[test]
    fn a_posterize_block_parses_its_levels() {
        let adjustment = parse_adjustment(b"post", &[0, 4, 0, 0], 256).expect("posterize");
        assert_eq!(
            adjustment,
            Adjustment::Posterize {
                levels: 4,
                lut_size: 256
            }
        );
    }

    #[test]
    fn an_invert_block_parses_empty() {
        assert_eq!(
            parse_adjustment(b"nvrt", &[], 256),
            Some(Adjustment::Invert)
        );
    }

    #[test]
    fn a_levels_block_parses_five_records() {
        let mut data = 2u16.to_be_bytes().to_vec();
        for value in [
            0u16, 255, 0, 255, 100, // master, identity
            10, 240, 0, 255, 100, // red
            0, 255, 0, 255, 100, // green
            0, 255, 0, 255, 100, // blue
        ] {
            data.extend_from_slice(&value.to_be_bytes());
        }
        let adjustment = parse_adjustment(b"levl", &data, 256).expect("levels");
        let Adjustment::Levels {
            master,
            per_channel,
        } = adjustment
        else {
            panic!("levels");
        };
        assert!((master.input_white - 1.0).abs() < 1e-6);
        assert!((per_channel[0].input_black - 10.0 / 255.0).abs() < 1e-6);
        assert!((per_channel[1].input_black).abs() < 1e-6);
    }

    #[test]
    fn a_curves_block_parses_points_per_channel() {
        // Version 1, bitmap with master and red set.
        let mut data = vec![0u8];
        data.extend_from_slice(&1u16.to_be_bytes());
        data.extend_from_slice(&0b0000_0011u32.to_be_bytes());
        for points in [[(0u16, 0u16), (255, 128)], [(0, 0), (255, 200)]] {
            data.extend_from_slice(&u16::try_from(points.len()).expect("len").to_be_bytes());
            for (input, output) in points {
                data.extend_from_slice(&output.to_be_bytes());
                data.extend_from_slice(&input.to_be_bytes());
            }
        }
        let adjustment = parse_adjustment(b"curv", &data, 256).expect("curves");
        let Adjustment::Curves {
            master,
            per_channel,
        } = adjustment
        else {
            panic!("curves");
        };
        assert!(
            master.get(1.0) < 0.6 && master.get(1.0) > 0.45,
            "{}",
            master.get(1.0)
        );
        assert!(per_channel[0].get(1.0) > 0.7, "{}", per_channel[0].get(1.0));
        // The green and blue channels were absent, so they are the identity.
        assert!((per_channel[1].get(0.5) - 0.5).abs() < 1e-3);
    }

    #[test]
    fn malformed_adjustment_blocks_return_none() {
        assert!(parse_adjustment(b"levl", &[0, 2, 1], 256).is_none());
        assert!(parse_adjustment(b"curv", &[0, 0, 9], 256).is_none());
        assert!(parse_adjustment(b"thrs", &[], 256).is_none());
        assert!(parse_adjustment(b"zzzz", &[0, 1], 256).is_none());
        assert!(parse_brightness_contrast(&[], None).is_none());
        assert!(parse_brightness_contrast(&[0, 10, 0xff, 0xf6], None).is_some());
    }

    #[test]
    fn a_brit_block_falls_back_to_legacy_values() {
        let adjustment =
            parse_brightness_contrast(&[0, 10, 0xff, 0xf6], None).expect("brightness/contrast");
        assert_eq!(
            adjustment,
            Adjustment::BrightnessContrast {
                brightness: 10.0,
                contrast: -10.0,
                legacy: true
            }
        );
    }
}
