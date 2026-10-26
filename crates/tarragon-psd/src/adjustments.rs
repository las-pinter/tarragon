//! Adjustment-layer math for the compositor.
//!
//! The core set covers invert, posterize, threshold, levels, curves and
//! brightness/contrast. Exposure, Channel Mixer, Color Balance, Hue/Saturation,
//! Selective Color and Photo Filter have their math in `crate::adjustment_color`.
//! Vibrance, Black & White and Gradient Map have their math in
//! `crate::adjustment_tone`.
//! Binary block parsing lives in `crate::adjustment_parse` and is re-exported here.
//!
//! Values are normalized `f32` in `0.0..=1.0`. Color Lookup and the Gradient
//! Map dither are documented gaps.

pub use crate::adjustment_color::{HueMaster, HueRange};
pub use crate::adjustment_parse::{parse_adjustment, parse_brightness_contrast};
use crate::{adjustment_color, adjustment_tone};

/// The number of samples in a tone lookup table.
const LUT_SIZE: usize = 256;

/// The Rec. 601 grey weights, used by Threshold.
const GREY_WEIGHTS: [f32; 3] = [0.299, 0.587, 0.114];

/// The PDF luminance weights, used by the non-separable blend modes.
const LUMINANCE_WEIGHTS: [f32; 3] = [0.3, 0.59, 0.11];

/// The pivot of the legacy Brightness/Contrast formula.
const LEGACY_PIVOT: f32 = 128.0 / 255.0;

/// One channel of a levels adjustment.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LevelsChannel {
    /// The input black point in `0.0..=1.0`.
    pub input_black: f32,
    /// The input white point in `0.0..=1.0`.
    pub input_white: f32,
    /// The midtone gamma in `0.01..=9.99`.
    pub gamma: f32,
    /// The output black point in `0.0..=1.0`.
    pub output_black: f32,
    /// The output white point in `0.0..=1.0`.
    pub output_white: f32,
}

impl LevelsChannel {
    /// An identity levels channel.
    #[must_use]
    pub const fn identity() -> Self {
        Self {
            input_black: 0.0,
            input_white: 1.0,
            gamma: 1.0,
            output_black: 0.0,
            output_white: 1.0,
        }
    }

    /// Apply the levels curve to one normalized value.
    #[must_use]
    pub fn apply(self, value: f32) -> f32 {
        let span = self.input_white - self.input_black;
        let scale = if span.abs() > 1e-9 { span } else { 1.0 };
        let base = ((value - self.input_black) / scale).clamp(0.0, 1.0);
        let gamma = self.gamma.clamp(0.01, 9.99);
        let shaped = levels_shape(base, gamma);
        (shaped * (self.output_white - self.output_black) + self.output_black).clamp(0.0, 1.0)
    }
}

/// The midtone gamma shape, with Photoshop's soft shadow toe for `gamma > 1`.
///
/// A plain `base^(1/gamma)` has infinite slope at black; Photoshop bounds it with a
/// linear toe (initial slope about `2^gamma`). A soft-min of the power curve and that
/// line keeps the exact power curve in the body and lifts only the darkest values.
/// `gamma <= 1` is untouched.
#[must_use]
fn levels_shape(base: f32, gamma: f32) -> f32 {
    if gamma <= 1.0 {
        return base.powf(1.0 / gamma);
    }
    let power = base.powf(1.0 / gamma);
    let line = 0.93 * 2.0f32.powf(gamma) * base;
    if power <= 1e-6 || line <= 1e-6 {
        return power.min(line);
    }
    // A p-norm soft-min (p = 10), fitted to whole-level input.
    (power.powf(-10.0) + line.powf(-10.0)).powf(-0.1)
}

/// One control point of a curve.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CurvePoint {
    /// The input value in `0.0..=1.0`.
    pub input: f32,
    /// The output value in `0.0..=1.0`.
    pub output: f32,
}

/// A tone curve: control points and a sampled lookup table.
#[derive(Debug, Clone, PartialEq)]
pub struct Curve {
    /// The control points, sorted by input.
    pub points: Vec<CurvePoint>,
    /// The sampled lookup table, `LUT_SIZE` entries.
    pub lut: Vec<f32>,
}

impl Curve {
    /// Build a curve from control points.
    ///
    /// A natural cubic spline runs through the points. Fewer than two points
    /// give the identity.
    #[must_use]
    pub fn new(mut points: Vec<CurvePoint>) -> Self {
        points.sort_by(|a, b| a.input.total_cmp(&b.input));
        points.dedup_by(|a, b| (a.input - b.input).abs() < 1e-6);
        let lut = build_curve_lut(&points);
        Self { points, lut }
    }

    /// The identity curve.
    #[must_use]
    pub fn identity() -> Self {
        Self::new(Vec::new())
    }

    /// The curve output at `value`, with linear interpolation between samples.
    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a 256-entry table fits every index exactly"
    )]
    #[must_use]
    pub fn get(&self, value: f32) -> f32 {
        let scaled = value.clamp(0.0, 1.0) * (LUT_SIZE - 1) as f32;
        let index = scaled.floor() as usize;
        let next = (index + 1).min(LUT_SIZE - 1);
        let fraction = scaled - index as f32;
        self.lut[index] * (1.0 - fraction) + self.lut[next] * fraction
    }
}

/// A core adjustment layer.
#[derive(Debug, Clone, PartialEq)]
pub enum Adjustment {
    /// Invert every color channel.
    Invert,
    /// Posterize to a number of levels in `2..=255`.
    Posterize {
        /// The number of output levels.
        levels: u8,
        /// The sample count, 256 for 8-bit and 65536 for 16-bit.
        lut_size: u32,
    },
    /// Threshold the grey value to black or white.
    Threshold {
        /// The grey threshold in `0.0..=1.0`.
        level: f32,
    },
    /// Apply a per-channel levels curve, then the master curve.
    Levels {
        /// The master curve.
        master: LevelsChannel,
        /// One curve per RGB channel.
        per_channel: [LevelsChannel; 3],
    },
    /// Apply a per-channel tone curve, then the master curve.
    Curves {
        /// The master curve.
        master: Curve,
        /// One curve per RGB channel.
        per_channel: [Curve; 3],
    },
    /// Adjust brightness and contrast.
    BrightnessContrast {
        /// The brightness in `-150.0..=150.0` (legacy: `-255.0..=255.0`).
        brightness: f32,
        /// The contrast in `-100.0..=100.0`.
        contrast: f32,
        /// True for the pre-CS3 legacy formula.
        legacy: bool,
    },
    /// Scale in linear light, add an offset, apply a gamma.
    Exposure {
        /// The exposure in stops.
        exposure: f32,
        /// The offset added in linear light.
        offset: f32,
        /// The gamma; the result is raised to its inverse.
        gamma: f32,
    },
    /// Mix the channels. Each row is red, green, blue and a constant, as fractions.
    ChannelMixer {
        /// One row per output channel.
        matrix: [[f32; 4]; 3],
        /// True to write the first row to every channel.
        monochrome: bool,
    },
    /// Shift the color balance of the shadows, midtones and highlights.
    ColorBalance {
        /// Shadows, midtones, highlights; each cyan-red, magenta-green, yellow-blue in percent.
        tones: [[f32; 3]; 3],
        /// True to keep the luminosity of the pixel.
        preserve_luminosity: bool,
    },
    /// Hue/Saturation with the master and six range edits.
    HueSaturation {
        /// The master controls.
        master: HueMaster,
        /// Reds, yellows, greens, cyans, blues, magentas.
        ranges: [HueRange; 6],
    },
    /// Selective Color.
    SelectiveColor {
        /// True for relative mode, false for absolute mode.
        relative: bool,
        /// Reds, yellows, greens, cyans, blues, magentas, whites, neutrals, blacks;
        /// each cyan, magenta, yellow, black in percent.
        records: [[f32; 4]; 9],
    },
    /// Tint the image with a filter color.
    PhotoFilter {
        /// The filter color in D50 XYZ.
        filter_xyz: [f32; 3],
        /// The density as a fraction.
        density: f32,
        /// True to keep the luminosity of the pixel.
        preserve_luminosity: bool,
    },
    /// Vibrance and Saturation.
    Vibrance {
        /// The vibrance slider in `-100.0..=100.0`.
        vibrance: f32,
        /// The saturation slider in `-100.0..=100.0`.
        saturation: f32,
    },
    /// Black & White, with an optional tint.
    BlackWhite {
        /// Reds, yellows, greens, cyans, blues, magentas, in percent.
        weights: [f32; 6],
        /// The tint color, or `None` for plain grey.
        tint: Option<[f32; 3]>,
    },
    /// Map the grey value of a pixel to a color on a gradient.
    ///
    /// The map is opaque, so the stop alpha is ignored. The dither flag and
    /// the smoothness of the block are not applied.
    GradientMap {
        /// The color stops, sorted by location.
        stops: Vec<(f32, [f32; 4])>,
        /// One midpoint per segment between the stops.
        midpoints: Vec<f32>,
        /// True to run the ramp from white to black.
        reverse: bool,
    },
}

impl Adjustment {
    /// Apply the adjustment to one RGB pixel.
    ///
    /// The caller blends the result with the original color by the layer
    /// opacity. Alpha is not part of this call; an adjustment keeps alpha.
    pub fn apply(&self, rgb: &mut [f32; 3]) {
        match self {
            Self::Invert => {
                for channel in &mut *rgb {
                    *channel = 1.0 - *channel;
                }
            }
            Self::Posterize { levels, lut_size } => {
                let levels = u32::from((*levels).clamp(2, 255));
                for channel in &mut *rgb {
                    *channel = posterize(*channel, levels, *lut_size);
                }
            }
            Self::Threshold { level } => {
                let grey = grey(*rgb);
                let threshold = (level * 255.0).round();
                let value = if (grey * 255.0).round() >= threshold {
                    1.0
                } else {
                    0.0
                };
                *rgb = [value, value, value];
            }
            Self::Levels {
                master,
                per_channel,
            } => {
                for (index, channel) in per_channel.iter().enumerate() {
                    rgb[index] = master.apply(channel.apply(rgb[index]));
                }
            }
            Self::Curves {
                master,
                per_channel,
            } => {
                for (index, curve) in per_channel.iter().enumerate() {
                    rgb[index] = master.get(curve.get(rgb[index]));
                }
            }
            Self::BrightnessContrast {
                brightness,
                contrast,
                legacy,
            } => {
                for channel in &mut *rgb {
                    *channel = if *legacy {
                        legacy_brightness_contrast(*channel, *brightness, *contrast)
                    } else {
                        modern_contrast(modern_brightness(*channel, *brightness), *contrast)
                            .clamp(0.0, 1.0)
                    };
                }
            }
            Self::Exposure {
                exposure,
                offset,
                gamma,
            } => *rgb = adjustment_color::exposure(*rgb, *exposure, *offset, *gamma),
            Self::ChannelMixer { matrix, monochrome } => {
                *rgb = adjustment_color::channel_mixer(*rgb, matrix, *monochrome);
            }
            Self::ColorBalance {
                tones,
                preserve_luminosity,
            } => *rgb = adjustment_color::color_balance(*rgb, tones, *preserve_luminosity),
            Self::HueSaturation { master, ranges } => {
                *rgb = adjustment_color::hue_saturation(*rgb, *master, ranges);
            }
            Self::SelectiveColor { relative, records } => {
                *rgb = adjustment_color::selective_color(*rgb, *relative, records);
            }
            Self::PhotoFilter {
                filter_xyz,
                density,
                preserve_luminosity,
            } => {
                *rgb = adjustment_color::photo_filter(
                    *rgb,
                    *filter_xyz,
                    *density,
                    *preserve_luminosity,
                );
            }
            Self::Vibrance {
                vibrance,
                saturation,
            } => *rgb = adjustment_tone::vibrance(*rgb, vibrance / 100.0, saturation / 100.0),
            Self::BlackWhite { weights, tint } => {
                *rgb = adjustment_tone::black_white(*rgb, weights, *tint);
            }
            Self::GradientMap {
                stops,
                midpoints,
                reverse,
            } => *rgb = adjustment_tone::gradient_map(*rgb, stops, midpoints, *reverse),
        }
    }
}

/// The PDF luminance weight of an RGB pixel.
#[must_use]
pub fn luminance(rgb: [f32; 3]) -> f32 {
    LUMINANCE_WEIGHTS[0] * rgb[0] + LUMINANCE_WEIGHTS[1] * rgb[1] + LUMINANCE_WEIGHTS[2] * rgb[2]
}

/// The Rec. 601 grey value of an RGB pixel.
#[must_use]
pub fn grey(rgb: [f32; 3]) -> f32 {
    GREY_WEIGHTS[0] * rgb[0] + GREY_WEIGHTS[1] * rgb[1] + GREY_WEIGHTS[2] * rgb[2]
}

/// Photoshop posterize: a floor over `levels` bins, sampled at the document depth.
#[allow(
    clippy::cast_precision_loss,
    reason = "the sample count is a small integer"
)]
#[must_use]
fn posterize(value: f32, levels: u32, lut_size: u32) -> f32 {
    let samples = lut_size.clamp(2, 65_536) as f32;
    let levels = (levels as f32).max(2.0);
    let max = samples - 1.0;
    let scaled = (value.clamp(0.0, 1.0) * max).round();
    let bin = (scaled * levels / samples).floor().min(levels - 1.0);
    (bin * max / (levels - 1.0)).floor() / max
}

/// The legacy Brightness/Contrast curve.
#[must_use]
fn legacy_brightness_contrast(value: f32, brightness: f32, contrast: f32) -> f32 {
    let brightness = brightness / 255.0;
    let contrast = contrast.clamp(-100.0, 99.0);
    let scale = if contrast >= 0.0 {
        1.0 / (1.0 - contrast / 100.0)
    } else {
        1.0 + contrast / 100.0
    };
    // Contrast raised: brightness comes first. Contrast lowered: it comes last.
    let (pre, post) = if contrast >= 0.0 {
        (brightness, 0.0)
    } else {
        (0.0, brightness)
    };
    ((value + pre - LEGACY_PIVOT) * scale + LEGACY_PIVOT + post).clamp(0.0, 1.0)
}

/// The modern (CS3+) brightness curve: a line that rolls off to white.
#[must_use]
fn modern_brightness(value: f32, brightness: f32) -> f32 {
    if brightness == 0.0 {
        return value;
    }
    let scale = 1.375f32.powf(brightness / 50.0);
    let power = if brightness >= 0.0 {
        (4.5 - 0.013 * brightness).max(2.0)
    } else {
        5.0 - 0.072 * brightness
    };
    let value = value.clamp(0.0, 1.0);
    (scale * value + (1.0 - scale) * value.powf(power)).clamp(0.0, 1.0)
}

/// The modern (CS3+) contrast curve: a symmetric cubic S-curve at 0.5.
#[must_use]
fn modern_contrast(value: f32, contrast: f32) -> f32 {
    if contrast == 0.0 {
        return value;
    }
    let k = contrast / 128.0;
    let (end, mid) = (1.0 - k, 1.0 + k);
    let half = |t: f32| {
        end * 0.5 * (t * t * t - 2.0 * t * t + t)
            + f32::midpoint(-2.0 * t * t * t, 3.0 * t * t)
            + mid * 0.5 * (t * t * t - t * t)
    };
    let value = value.clamp(0.0, 1.0);
    if value <= 0.5 {
        half(value / 0.5)
    } else {
        1.0 - half((1.0 - value) / 0.5)
    }
}

/// Sample a natural cubic spline through `points` into a lookup table.
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::many_single_char_names,
    reason = "the spline solve uses the standard single-letter names"
)]
#[must_use]
fn build_curve_lut(points: &[CurvePoint]) -> Vec<f32> {
    let identity = |k: usize| k as f32 / (LUT_SIZE - 1) as f32;
    if points.len() < 2 {
        return (0..LUT_SIZE).map(identity).collect();
    }
    let n = points.len();
    let x: Vec<f64> = points.iter().map(|p| f64::from(p.input)).collect();
    let y: Vec<f64> = points.iter().map(|p| f64::from(p.output)).collect();
    let mut second = vec![0.0f64; n];
    if n > 2 {
        let mut c = vec![0.0f64; n];
        let mut d = vec![0.0f64; n];
        for i in 1..n - 1 {
            let (h0, h1) = (x[i] - x[i - 1], x[i + 1] - x[i]);
            let a = h0 / 6.0;
            let b = (h0 + h1) / 3.0;
            let cc = h1 / 6.0;
            let r = (y[i + 1] - y[i]) / h1 - (y[i] - y[i - 1]) / h0;
            let denom = b - a * c[i - 1];
            c[i] = cc / denom;
            d[i] = (r - a * d[i - 1]) / denom;
        }
        for i in (1..n - 1).rev() {
            second[i] = d[i] - c[i] * second[i + 1];
        }
    }
    (0..LUT_SIZE)
        .map(|k| {
            let value = identity(k);
            if value <= points[0].input {
                return points[0].output.clamp(0.0, 1.0);
            }
            if value >= points[n - 1].input {
                return points[n - 1].output.clamp(0.0, 1.0);
            }
            let index = points
                .windows(2)
                .position(|w| value <= w[1].input)
                .unwrap_or(n - 2);
            let (x0, y0) = (x[index], y[index]);
            let (x1, y1) = (x[index + 1], y[index + 1]);
            let h = x1 - x0;
            let t = f64::from(value);
            let a = (x1 - t) / h;
            let b = (t - x0) / h;
            let out = a * y0
                + b * y1
                + ((a * a * a - a) * second[index] + (b * b * b - b) * second[index + 1]) * h * h
                    / 6.0;
            (out as f32).clamp(0.0, 1.0)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{Adjustment, Curve, CurvePoint, LevelsChannel, posterize};

    #[test]
    fn invert_is_the_unit_complement() {
        let mut rgb = [0.25, 0.5, 0.75];
        Adjustment::Invert.apply(&mut rgb);
        assert!((rgb[0] - 0.75).abs() < 1e-6);
        assert!((rgb[1] - 0.5).abs() < 1e-6);
        assert!((rgb[2] - 0.25).abs() < 1e-6);
    }

    #[test]
    fn an_identity_levels_channel_changes_nothing() {
        let curve = LevelsChannel::identity();
        for value in [0.0, 0.25, 0.5, 0.75, 1.0] {
            assert!((curve.apply(value) - value).abs() < 1e-6, "{value}");
        }
    }

    #[test]
    fn a_clipped_levels_channel_maps_the_range() {
        let curve = LevelsChannel {
            input_black: 0.5,
            input_white: 1.0,
            gamma: 1.0,
            output_black: 0.0,
            output_white: 1.0,
        };
        assert!((curve.apply(0.5) - 0.0).abs() < 1e-6);
        assert!((curve.apply(1.0) - 1.0).abs() < 1e-6);
        assert!((curve.apply(0.75) - 0.5).abs() < 1e-6);
        assert!((curve.apply(0.2) - 0.0).abs() < 1e-6);
    }

    #[test]
    fn a_levels_gamma_above_one_lifts_the_shadows_with_a_soft_toe() {
        let curve = LevelsChannel {
            gamma: 2.0,
            ..LevelsChannel::identity()
        };
        // A pure power curve gives sqrt(0.01) = 0.1; the toe bounds the input to about
        // 3.72x near black (initial slope ~ 2^gamma).
        let lifted = curve.apply(0.01);
        assert!(lifted < 0.1, "{lifted}");
        assert!((lifted - 0.0372).abs() < 1e-3, "{lifted}");
        // Gamma at or below one keeps the plain power curve.
        assert!((LevelsChannel::identity().apply(0.25) - 0.25).abs() < 1e-6);
    }

    #[test]
    fn levels_apply_the_channel_then_the_master() {
        let adjustment = Adjustment::Levels {
            master: LevelsChannel {
                output_white: 0.5,
                ..LevelsChannel::identity()
            },
            per_channel: [LevelsChannel {
                output_black: 0.5,
                ..LevelsChannel::identity()
            }; 3],
        };
        // channel(0) = 0.5, then master(0.5) = 0.25.
        let mut rgb = [0.0, 0.0, 0.0];
        adjustment.apply(&mut rgb);
        assert!((rgb[0] - 0.25).abs() < 1e-6, "{rgb:?}");
    }

    #[test]
    fn a_line_curve_is_the_identity() {
        let curve = Curve::new(vec![
            CurvePoint {
                input: 0.0,
                output: 0.0,
            },
            CurvePoint {
                input: 1.0,
                output: 1.0,
            },
        ]);
        for value in [0.0, 0.25, 0.5, 0.75, 1.0] {
            assert!((curve.get(value) - value).abs() < 1e-4, "{value}");
        }
    }

    #[test]
    fn a_lifted_curve_raises_the_midtones() {
        let curve = Curve::new(vec![
            CurvePoint {
                input: 0.0,
                output: 0.0,
            },
            CurvePoint {
                input: 0.5,
                output: 0.75,
            },
            CurvePoint {
                input: 1.0,
                output: 1.0,
            },
        ]);
        assert!(curve.get(0.5) > 0.7, "{}", curve.get(0.5));
        assert!(curve.get(0.0).abs() < 1e-4);
        assert!((curve.get(1.0) - 1.0).abs() < 1e-4);
    }

    #[test]
    fn curves_apply_the_channel_then_the_master() {
        // Both curves halve the value, so 1.0 maps through 0.5 then 0.25.
        let halve = vec![
            CurvePoint {
                input: 0.0,
                output: 0.0,
            },
            CurvePoint {
                input: 1.0,
                output: 0.5,
            },
        ];
        let adjustment = Adjustment::Curves {
            master: Curve::new(halve.clone()),
            per_channel: [
                Curve::new(halve.clone()),
                Curve::new(halve.clone()),
                Curve::new(halve),
            ],
        };
        let mut rgb = [1.0, 1.0, 1.0];
        adjustment.apply(&mut rgb);
        assert!((rgb[0] - 0.25).abs() < 1e-3, "{rgb:?}");
    }

    #[test]
    fn a_two_level_posterize_rounds_to_black_or_white() {
        assert!((posterize(0.1, 2, 256)).abs() < 1e-6);
        assert!((posterize(0.9, 2, 256) - 1.0).abs() < 1e-6);
        // A known four-level value: 0.5 maps to 170/255.
        assert!((posterize(0.5, 4, 256) - 170.0 / 255.0).abs() < 1e-6);
    }

    #[test]
    fn a_threshold_of_half_splits_on_grey() {
        let adjustment = Adjustment::Threshold { level: 0.5 };
        let mut dark = [0.1, 0.1, 0.1];
        adjustment.apply(&mut dark);
        assert_eq!(dark, [0.0, 0.0, 0.0]);
        let mut light = [0.9, 0.9, 0.9];
        adjustment.apply(&mut light);
        assert_eq!(light, [1.0, 1.0, 1.0]);
    }

    #[test]
    fn legacy_brightness_contrast_moves_the_midtone() {
        let adjustment = Adjustment::BrightnessContrast {
            brightness: 40.0,
            contrast: 0.0,
            legacy: true,
        };
        let mut rgb = [128.0 / 255.0; 3];
        adjustment.apply(&mut rgb);
        // 128/255 + 40/255 = 168/255.
        assert!((rgb[0] - 168.0 / 255.0).abs() < 1e-6, "{rgb:?}");
        // Pure black and white stay put at zero settings.
        let identity = Adjustment::BrightnessContrast {
            brightness: 0.0,
            contrast: 0.0,
            legacy: true,
        };
        let mut black = [0.0, 0.0, 0.0];
        identity.apply(&mut black);
        assert_eq!(black, [0.0, 0.0, 0.0]);
        let mut white = [1.0, 1.0, 1.0];
        identity.apply(&mut white);
        assert_eq!(white, [1.0, 1.0, 1.0]);
    }
}
