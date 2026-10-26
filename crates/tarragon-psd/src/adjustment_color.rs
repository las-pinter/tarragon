//! Color adjustment math: Exposure, Channel Mixer, Color Balance,
//! Hue/Saturation, Selective Color and Photo Filter.
//!
//! Every function maps one RGB pixel in `0.0..=1.0`. The formulas are
//! approximations of Photoshop. They are exact at the neutral setting of each
//! control and follow the documented behavior elsewhere. The functions use only
//! `f32` math, so an 8-bit and a 16-bit document give the same result.

use crate::adjustments::grey;
use crate::blend::{BlendMode, blend_rgb};

/// The D50 white point in XYZ.
const D50: [f32; 3] = [0.964_22, 1.0, 0.825_21];

/// Linear sRGB to D50 XYZ.
const SRGB_TO_XYZ_D50: [[f32; 3]; 3] = [
    [0.436_074, 0.385_064, 0.143_080],
    [0.222_504, 0.716_878, 0.060_618],
    [0.013_932, 0.097_104, 0.714_173],
];

/// D50 XYZ to linear sRGB.
const XYZ_D50_TO_SRGB: [[f32; 3]; 3] = [
    [3.133_856, -1.616_867, -0.490_615],
    [-0.978_768, 1.916_142, 0.033_454],
    [0.071_945, -0.228_991, 1.405_243],
];

/// The tone curve that Exposure works through: a pure power law.
const EXPOSURE_CURVE: f32 = 2.2;

/// Multiply a row-major 3 by 3 matrix with a vector.
fn mat3_mul(matrix: &[[f32; 3]; 3], vector: [f32; 3]) -> [f32; 3] {
    matrix.map(|row| row[0] * vector[0] + row[1] * vector[1] + row[2] * vector[2])
}

/// The sRGB curve, encoded to linear.
pub(crate) fn srgb_to_linear(value: f32) -> f32 {
    let value = value.max(0.0);
    if value <= 0.040_45 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

/// The sRGB curve, linear to encoded.
pub(crate) fn linear_to_srgb(value: f32) -> f32 {
    let value = value.max(0.0);
    if value <= 0.003_130_8 {
        value * 12.92
    } else {
        1.055 * value.powf(1.0 / 2.4) - 0.055
    }
}

/// Exposure: scale in linear light, add the offset, apply the gamma.
#[must_use]
pub fn exposure(rgb: [f32; 3], exposure: f32, offset: f32, gamma: f32) -> [f32; 3] {
    // A huge stop count would give infinity, and then NaN on a black pixel.
    let scale = exposure.clamp(-32.0, 32.0).exp2();
    let gamma = gamma.max(0.01);
    rgb.map(|value| {
        let linear = (value.max(0.0).powf(EXPOSURE_CURVE) * scale + offset).max(0.0);
        let linear = if (gamma - 1.0).abs() < f32::EPSILON {
            linear
        } else {
            linear.powf(1.0 / gamma)
        };
        linear.powf(1.0 / EXPOSURE_CURVE).clamp(0.0, 1.0)
    })
}

/// Channel Mixer. Each row is `[red, green, blue, constant]` as fractions.
///
/// In monochrome mode the first row is the grey mix and goes to all channels.
#[must_use]
pub fn channel_mixer(rgb: [f32; 3], matrix: &[[f32; 4]; 3], monochrome: bool) -> [f32; 3] {
    let mix = |row: &[f32; 4]| {
        (row[0] * rgb[0] + row[1] * rgb[1] + row[2] * rgb[2] + row[3]).clamp(0.0, 1.0)
    };
    if monochrome {
        [mix(&matrix[0]); 3]
    } else {
        [mix(&matrix[0]), mix(&matrix[1]), mix(&matrix[2])]
    }
}

/// Color Balance. It uses the 0.299 grey on purpose; Photo Filter uses the
/// 0.3/0.59/0.11 luminosity of the Luminosity blend mode.
///
/// `tones` holds shadows, midtones and highlights, each as
/// cyan-red, magenta-green and yellow-blue in percent.
#[must_use]
pub fn color_balance(rgb: [f32; 3], tones: &[[f32; 3]; 3], preserve_luminosity: bool) -> [f32; 3] {
    let light = grey(rgb);
    let shadow_weight = (1.0 - 2.0 * light).clamp(0.0, 1.0);
    let highlight_weight = (2.0 * light - 1.0).clamp(0.0, 1.0);
    let mid_weight = 1.0 - shadow_weight - highlight_weight;
    let out: [f32; 3] = std::array::from_fn(|index| {
        let shift = tones[0][index] * shadow_weight
            + tones[1][index] * mid_weight
            + tones[2][index] * highlight_weight;
        (rgb[index] + shift / 100.0 * 0.5).clamp(0.0, 1.0)
    });
    if preserve_luminosity {
        let new_light = grey(out);
        if new_light > 1e-6 {
            return out.map(|value| (value * light / new_light).clamp(0.0, 1.0));
        }
    }
    out
}

/// One of the six color ranges of Hue/Saturation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HueRange {
    /// The hue shift in degrees.
    pub hue: f32,
    /// The saturation change in percent.
    pub saturation: f32,
    /// The lightness change in percent.
    pub lightness: f32,
    /// The hue degrees where the range starts to fade in, is fully on, starts
    /// to fade out and ends. The range can wrap past 0 and 360.
    pub bounds: [f32; 4],
}

impl HueRange {
    /// The neutral range `index` (0 reds, 1 yellows, and so on) with the
    /// default extent: 30 degrees fully on, fading over 30 degrees each side.
    #[allow(clippy::cast_precision_loss, reason = "the index is at most five")]
    #[must_use]
    pub fn neutral(index: usize) -> Self {
        let centre = 60.0 * index as f32;
        Self {
            hue: 0.0,
            saturation: 0.0,
            lightness: 0.0,
            bounds: [centre - 45.0, centre - 15.0, centre + 15.0, centre + 45.0],
        }
    }

    /// The six neutral ranges.
    #[must_use]
    pub fn defaults() -> [Self; 6] {
        std::array::from_fn(Self::neutral)
    }

    /// Order four stored bounds so they increase within one turn.
    ///
    /// A bad value gives the bounds of the default reds.
    #[must_use]
    pub fn canonical_bounds(bounds: [f32; 4]) -> [f32; 4] {
        if bounds.iter().any(|value| !value.is_finite()) {
            return [-45.0, -15.0, 15.0, 45.0];
        }
        let mut out = [bounds[0]; 4];
        for index in 1..4 {
            let step = (bounds[index] - out[index - 1]).rem_euclid(360.0);
            out[index] = (out[index - 1] + step).min(out[0] + 359.0);
        }
        // Keep the centre in -30..330, so the reds read as -45 to 45.
        let centre = f32::midpoint(out[1], out[2]);
        let shift = (centre + 30.0).rem_euclid(360.0) - 30.0 - centre;
        out.map(|value| value + shift)
    }

    /// True when the range changes nothing.
    #[must_use]
    pub fn is_neutral(&self) -> bool {
        self.hue == 0.0 && self.saturation == 0.0 && self.lightness == 0.0
    }

    /// How much a pixel of hue `degrees` belongs to this range, `0.0..=1.0`.
    #[must_use]
    pub fn weight(&self, degrees: f32) -> f32 {
        let [start, rise, fall, end] = self.bounds;
        // Measure from the start of the range, so a wrapping range works.
        let relative = |value: f32| (value - start).rem_euclid(360.0);
        let rise = relative(rise);
        let fall = relative(fall).max(rise);
        let end = relative(end).max(fall);
        let at = relative(degrees);
        if at <= rise {
            if rise <= 0.0 { 1.0 } else { at / rise }
        } else if at <= fall {
            1.0
        } else if at <= end {
            if end <= fall {
                1.0
            } else {
                (end - at) / (end - fall)
            }
        } else {
            0.0
        }
    }
}

/// The master controls of Hue/Saturation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HueMaster {
    /// The hue shift in degrees, or the hue to set when colorizing.
    pub hue: f32,
    /// The saturation change in percent, or the saturation to set.
    pub saturation: f32,
    /// The lightness change in percent.
    pub lightness: f32,
    /// True to colorize.
    pub colorize: bool,
}

/// Hue/Saturation with a master edit and six hue-range edits.
///
/// A range edit follows the original hue of the pixel. It fades out toward
/// grey, because a grey has no hue.
#[must_use]
pub fn hue_saturation(rgb: [f32; 3], master: HueMaster, ranges: &[HueRange; 6]) -> [f32; 3] {
    let (mut hue_shift, mut sat_shift, mut light_shift) = (0.0, 0.0, 0.0);
    if !master.colorize && ranges.iter().any(|range| !range.is_neutral()) {
        let degrees = rgb_to_hsl(rgb).0 * 360.0;
        let spread = rgb[0].max(rgb[1]).max(rgb[2]) - rgb[0].min(rgb[1]).min(rgb[2]);
        let fade = (spread * 4.0).min(1.0);
        for range in ranges.iter().filter(|range| !range.is_neutral()) {
            let weight = range.weight(degrees) * fade;
            hue_shift += weight * range.hue;
            sat_shift += weight * range.saturation / 100.0;
            light_shift += weight * range.lightness / 100.0;
        }
    }
    let hue = master.hue + hue_shift;
    let saturation = (master.saturation / 100.0 + sat_shift).clamp(-1.0, 1.0);
    let lightness = (master.lightness / 100.0 + light_shift).clamp(-1.0, 1.0);
    let (mut h, mut s, l) = rgb_to_hsl(rgb);
    if master.colorize {
        h = hue.rem_euclid(360.0) / 360.0;
        s = saturation.abs().min(1.0);
    } else {
        h = (h + hue / 360.0).rem_euclid(1.0);
        s = (s * (1.0 + saturation)).clamp(0.0, 1.0);
    }
    let out = hsl_to_rgb(h, s, l);
    if lightness > 0.0 {
        out.map(|value| value + (1.0 - value) * lightness)
    } else {
        out.map(|value| value * (1.0 + lightness))
    }
}

/// Convert RGB to hue (`0.0..1.0`), saturation and lightness.
fn rgb_to_hsl(rgb: [f32; 3]) -> (f32, f32, f32) {
    let max = rgb[0].max(rgb[1]).max(rgb[2]);
    let min = rgb[0].min(rgb[1]).min(rgb[2]);
    let light = f32::midpoint(max, min);
    let span = max - min;
    if span < 1e-7 {
        return (0.0, 0.0, light);
    }
    let saturation = if light > 0.5 {
        span / (2.0 - max - min)
    } else {
        span / (max + min)
    };
    let sector = if rgb[0] >= rgb[1] && rgb[0] >= rgb[2] {
        ((rgb[1] - rgb[2]) / span).rem_euclid(6.0)
    } else if rgb[1] >= rgb[2] {
        (rgb[2] - rgb[0]) / span + 2.0
    } else {
        (rgb[0] - rgb[1]) / span + 4.0
    };
    (sector / 6.0, saturation, light)
}

/// Convert hue (`0.0..1.0`), saturation and lightness to RGB.
fn hsl_to_rgb(hue: f32, saturation: f32, light: f32) -> [f32; 3] {
    if saturation <= 0.0 {
        return [light; 3];
    }
    let upper = if light < 0.5 {
        light * (1.0 + saturation)
    } else {
        light + saturation - light * saturation
    };
    let lower = 2.0 * light - upper;
    let channel = |offset: f32| {
        let t = (hue + offset).rem_euclid(1.0);
        if t < 1.0 / 6.0 {
            lower + (upper - lower) * 6.0 * t
        } else if t < 0.5 {
            upper
        } else if t < 2.0 / 3.0 {
            lower + (upper - lower) * (2.0 / 3.0 - t) * 6.0
        } else {
            lower
        }
    };
    [channel(1.0 / 3.0), channel(0.0), channel(-1.0 / 3.0)]
}

/// How much a color belongs to each Selective Color range: reds, yellows,
/// greens, cyans, blues and magentas by chroma in the hue sector, then whites,
/// neutrals and blacks by lightness.
fn selective_weights(rgb: [f32; 3]) -> [f32; 9] {
    let max = rgb[0].max(rgb[1]).max(rgb[2]);
    let min = rgb[0].min(rgb[1]).min(rgb[2]);
    let mid = rgb[0] + rgb[1] + rgb[2] - max - min;
    let top = |index: usize| if rgb[index] >= max { max - mid } else { 0.0 };
    let bottom = |index: usize| if rgb[index] <= min { mid - min } else { 0.0 };
    [
        top(0),
        bottom(2),
        top(1),
        bottom(0),
        top(2),
        bottom(1),
        ((min - 0.5) * 2.0).max(0.0),
        (1.0 - (max - 0.5).abs() - (min - 0.5).abs()).clamp(0.0, 1.0),
        ((0.5 - max) * 2.0).max(0.0),
    ]
}

/// Selective Color. Each record is `[cyan, magenta, yellow, black]` in percent.
///
/// A range moves the cyan, magenta and yellow ink (one minus red, green and
/// blue) by its percentage plus its black percentage, weighted by how much the
/// color belongs to the range. Relative mode scales the move by the ink that
/// is present. Absolute mode adds it as it is.
#[must_use]
pub fn selective_color(rgb: [f32; 3], relative: bool, records: &[[f32; 4]; 9]) -> [f32; 3] {
    let weights = selective_weights(rgb);
    let mut delta = [0.0f32; 3];
    for (record, weight) in records.iter().zip(weights) {
        if weight <= 0.0 {
            continue;
        }
        let shift = record.map(|value| value / 100.0);
        for index in 0..3 {
            let ink = 1.0 - rgb[index];
            let step = shift[index] + shift[3];
            delta[index] += if relative { step * ink } else { step } * weight;
        }
    }
    std::array::from_fn(|index| (rgb[index] - delta[index]).clamp(0.0, 1.0))
}

/// Photo Filter. `filter` is the filter color in D50 XYZ, `density` is a
/// fraction.
///
/// In linear light the pixel XYZ is scaled by
/// `1 - density + density * filter / white`. Preserve luminosity then restores
/// the original luminosity of the pixel.
#[must_use]
pub fn photo_filter(
    rgb: [f32; 3],
    filter: [f32; 3],
    density: f32,
    preserve_luminosity: bool,
) -> [f32; 3] {
    let density = if density.is_finite() {
        density.clamp(0.0, 1.0)
    } else {
        0.0
    };
    let linear = rgb.map(srgb_to_linear);
    let mut xyz = mat3_mul(&SRGB_TO_XYZ_D50, linear);
    for index in 0..3 {
        xyz[index] *= 1.0 - density + density * filter[index] / D50[index];
    }
    let filtered =
        mat3_mul(&XYZ_D50_TO_SRGB, xyz).map(|value| linear_to_srgb(value).clamp(0.0, 1.0));
    if preserve_luminosity {
        blend_rgb(BlendMode::Luminosity, filtered, rgb)
    } else {
        filtered
    }
}

/// The D50 XYZ of an sRGB color. Photo Filter version 2 stores its color as RGB.
#[must_use]
pub fn srgb_to_xyz(rgb: [f32; 3]) -> [f32; 3] {
    mat3_mul(&SRGB_TO_XYZ_D50, rgb.map(srgb_to_linear))
}

/// The D50 XYZ of a CIE Lab color with `l` in `0..=100`.
#[must_use]
pub fn lab_to_xyz(lab: [f32; 3]) -> [f32; 3] {
    let fy = (lab[0] + 16.0) / 116.0;
    let fx = fy + lab[1] / 500.0;
    let fz = fy - lab[2] / 200.0;
    let inverse = |t: f32| {
        if t > 6.0 / 29.0 {
            t * t * t
        } else {
            3.0 * (6.0f32 / 29.0).powi(2) * (t - 4.0 / 29.0)
        }
    };
    [
        inverse(fx) * D50[0],
        inverse(fy) * D50[1],
        inverse(fz) * D50[2],
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn neutral_settings_are_the_identity() {
        let pixel = [0.2, 0.5, 0.8];
        let close = |a: [f32; 3], b: [f32; 3]| a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-4);
        assert!(close(exposure(pixel, 0.0, 0.0, 1.0), pixel));
        assert!(close(color_balance(pixel, &[[0.0; 3]; 3], true), pixel));
        let master = HueMaster {
            hue: 0.0,
            saturation: 0.0,
            lightness: 0.0,
            colorize: false,
        };
        assert!(close(
            hue_saturation(pixel, master, &HueRange::defaults()),
            pixel
        ));
        assert!(close(selective_color(pixel, true, &[[0.0; 4]; 9]), pixel));
        assert!(close(
            photo_filter(pixel, [0.9, 0.8, 0.3], 0.0, true),
            pixel
        ));
    }

    #[test]
    fn a_range_weight_wraps_around_red() {
        let reds = HueRange::neutral(0);
        assert!((reds.weight(0.0) - 1.0).abs() < 1e-6);
        assert!((reds.weight(350.0) - 1.0).abs() < 1e-6);
        assert!((reds.weight(30.0) - 0.5).abs() < 1e-5);
        assert!(reds.weight(120.0).abs() < 1e-6);
    }

    #[test]
    fn bad_bounds_never_give_a_bad_weight() {
        let nan = HueRange {
            bounds: [f32::NAN; 4],
            ..HueRange::neutral(0)
        };
        assert!(nan.weight(10.0).is_finite());
        let flat = HueRange {
            bounds: [10.0; 4],
            ..HueRange::neutral(0)
        };
        assert!(flat.weight(10.0).is_finite() && flat.weight(200.0).is_finite());
    }

    #[test]
    fn white_lab_is_the_white_point() {
        let xyz = lab_to_xyz([100.0, 0.0, 0.0]);
        for (value, white) in xyz.iter().zip(D50) {
            assert!((value - white).abs() < 1e-4);
        }
    }
}
