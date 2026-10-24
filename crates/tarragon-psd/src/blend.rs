//! Photoshop blend modes for the layer compositor.
//!
//! The formulas follow Adobe's PDF blend-mode specification. Separable modes
//! run per color channel. Non-separable modes convert to a luminance and
//! saturation form. Dissolve is not a blend function; it falls back to normal.
//!
//! All values are normalized `f32` in `0.0..=1.0`.
//!
//! Vivid Light, Hard Mix, Color Burn and Color Dodge use Photoshop's variant,
//! where a source extreme wins and near-extreme backdrop values count as exact.

/// Backdrop values this close to 0 or 1 count as exact where the result jumps.
const EDGE: f32 = 1e-4;

/// One Photoshop layer blend mode, plus the group pass-through marker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlendMode {
    /// A pass-through group. It composites onto the backdrop, not a buffer.
    PassThrough,
    /// The default source-over mode.
    Normal,
    /// Dissolve. It is treated as normal.
    Dissolve,
    /// Darken.
    Darken,
    /// Multiply.
    Multiply,
    /// Color burn.
    ColorBurn,
    /// Linear burn.
    LinearBurn,
    /// Darker color (non-separable).
    DarkerColor,
    /// Lighten.
    Lighten,
    /// Screen.
    Screen,
    /// Color dodge.
    ColorDodge,
    /// Linear dodge (add).
    LinearDodge,
    /// Lighter color (non-separable).
    LighterColor,
    /// Overlay.
    Overlay,
    /// Soft light.
    SoftLight,
    /// Hard light.
    HardLight,
    /// Vivid light.
    VividLight,
    /// Linear light.
    LinearLight,
    /// Pin light.
    PinLight,
    /// Hard mix.
    HardMix,
    /// Difference.
    Difference,
    /// Exclusion.
    Exclusion,
    /// Subtract.
    Subtract,
    /// Divide.
    Divide,
    /// Hue (non-separable).
    Hue,
    /// Saturation (non-separable).
    Saturation,
    /// Color (non-separable).
    Color,
    /// Luminosity (non-separable).
    Luminosity,
}

impl BlendMode {
    /// Map the four blend-key bytes of a layer record to a [`BlendMode`].
    ///
    /// An unknown key falls back to normal.
    #[must_use]
    pub const fn from_key(key: [u8; 4]) -> Self {
        match &key {
            b"pass" => Self::PassThrough,
            b"diss" => Self::Dissolve,
            b"dark" => Self::Darken,
            b"mul " => Self::Multiply,
            b"idiv" => Self::ColorBurn,
            b"lbrn" => Self::LinearBurn,
            b"dkCl" => Self::DarkerColor,
            b"lite" => Self::Lighten,
            b"scrn" => Self::Screen,
            b"div " => Self::ColorDodge,
            b"lddg" => Self::LinearDodge,
            b"lgCl" => Self::LighterColor,
            b"over" => Self::Overlay,
            b"sLit" => Self::SoftLight,
            b"hLit" => Self::HardLight,
            b"vLit" => Self::VividLight,
            b"lLit" => Self::LinearLight,
            b"pLit" => Self::PinLight,
            b"hMix" => Self::HardMix,
            b"diff" => Self::Difference,
            b"smud" => Self::Exclusion,
            b"fsub" => Self::Subtract,
            b"fdiv" => Self::Divide,
            b"hue " => Self::Hue,
            b"sat " => Self::Saturation,
            b"colr" => Self::Color,
            b"lum " => Self::Luminosity,
            // `norm` and any unknown key fall back to normal.
            _ => Self::Normal,
        }
    }
}

/// Clamp a value to `0.0..=1.0`.
#[inline]
#[must_use]
fn clamp01(value: f32) -> f32 {
    value.clamp(0.0, 1.0)
}

/// The union of two coverage or alpha values: `a + b - a * b`.
#[inline]
#[must_use]
pub(crate) fn union(a: f32, b: f32) -> f32 {
    a + b - a * b
}

/// A division that returns `1.0` when the result is not finite.
#[inline]
#[must_use]
pub(crate) fn safe_div(a: f32, b: f32) -> f32 {
    let c = a / b;
    if c.is_finite() { c } else { 1.0 }
}

/// One separable blend channel. `cb` is the backdrop, `cs` the source.
#[must_use]
fn separable(mode: BlendMode, cb: f32, cs: f32) -> f32 {
    match mode {
        BlendMode::Normal
        | BlendMode::Dissolve
        | BlendMode::PassThrough
        | BlendMode::DarkerColor
        | BlendMode::LighterColor
        | BlendMode::Hue
        | BlendMode::Saturation
        | BlendMode::Color
        | BlendMode::Luminosity => cs,
        BlendMode::Darken => cb.min(cs),
        BlendMode::Multiply => cb * cs,
        BlendMode::ColorBurn => color_burn(cb, cs),
        BlendMode::LinearBurn => (cb + cs - 1.0).max(0.0),
        BlendMode::Lighten => cb.max(cs),
        BlendMode::Screen => cb + cs - cb * cs,
        BlendMode::ColorDodge => color_dodge(cb, cs),
        BlendMode::LinearDodge => (cb + cs).min(1.0),
        BlendMode::Overlay => hard_light(cs, cb),
        BlendMode::SoftLight => soft_light(cb, cs),
        BlendMode::HardLight => hard_light(cb, cs),
        BlendMode::VividLight => vivid_light(cb, cs),
        BlendMode::LinearLight => (cb + 2.0 * cs - 1.0).clamp(0.0, 1.0),
        BlendMode::PinLight => {
            if cs > 0.5 {
                cb.max(2.0 * cs - 1.0)
            } else {
                cb.min(2.0 * cs)
            }
        }
        BlendMode::HardMix => hard_mix(cb, cs),
        BlendMode::Difference => (cb - cs).abs(),
        BlendMode::Exclusion => cb + cs - 2.0 * cb * cs,
        BlendMode::Subtract => (cb - cs).max(0.0),
        BlendMode::Divide => {
            if cs <= 0.0 {
                if cb <= 0.0 { 0.0 } else { 1.0 }
            } else {
                (cb / cs).min(1.0)
            }
        }
    }
}

/// Color Burn with Photoshop's near-extreme backdrop rule.
#[must_use]
fn color_burn(cb: f32, cs: f32) -> f32 {
    if cb >= 1.0 - EDGE {
        1.0
    } else if cs <= 0.0 {
        0.0
    } else {
        1.0 - ((1.0 - cb) / cs).min(1.0)
    }
}

/// Color Dodge with Photoshop's near-extreme backdrop rule.
#[must_use]
fn color_dodge(cb: f32, cs: f32) -> f32 {
    if cb <= EDGE {
        0.0
    } else if cs >= 1.0 {
        1.0
    } else {
        (cb / (1.0 - cs)).min(1.0)
    }
}

/// Vivid Light: a source extreme wins over any backdrop.
#[must_use]
fn vivid_light(cb: f32, cs: f32) -> f32 {
    if cs <= 0.5 {
        if cs <= 0.0 {
            0.0
        } else {
            1.0 - ((1.0 - cb) / (2.0 * cs)).min(1.0)
        }
    } else if cs >= 1.0 {
        1.0
    } else {
        (cb / (2.0 * (1.0 - cs))).min(1.0)
    }
}

/// Vivid Light for the Hard Mix threshold: backdrop extremes win there.
#[must_use]
fn vivid_light_for_hard_mix(cb: f32, cs: f32) -> f32 {
    if cs <= 0.5 {
        if cb >= 1.0 - EDGE {
            1.0
        } else if cs <= 0.0 {
            0.0
        } else {
            1.0 - ((1.0 - cb) / (2.0 * cs)).min(1.0)
        }
    } else if cb <= EDGE {
        0.0
    } else if cs >= 1.0 {
        1.0
    } else {
        (cb / (2.0 * (1.0 - cs))).min(1.0)
    }
}

/// Hard Mix: the hard-thresholded Vivid Light.
#[must_use]
fn hard_mix(cb: f32, cs: f32) -> f32 {
    if vivid_light_for_hard_mix(cb, cs) >= 0.5 - 1e-6 {
        1.0
    } else {
        0.0
    }
}

/// Hard light on one channel.
#[must_use]
fn hard_light(cb: f32, cs: f32) -> f32 {
    if cs > 0.5 {
        let b = 2.0 * cs - 1.0;
        cb + b - cb * b
    } else {
        cb * (2.0 * cs)
    }
}

/// Soft light on one channel.
///
/// The `cs > 0.5` branch uses `sqrt(cb)` for every backdrop. The PDF/W3C form
/// switches to a polynomial `D(cb)` when `cb <= 0.25`; this one does not.
#[must_use]
fn soft_light(cb: f32, cs: f32) -> f32 {
    let d = cb.sqrt();
    if cs <= 0.5 {
        cb - (1.0 - 2.0 * cs) * cb * (1.0 - cb)
    } else {
        cb + (2.0 * cs - 1.0) * (d - cb)
    }
}

/// The luminance weight of the PDF blend specification.
#[must_use]
fn luminance(c: [f32; 3]) -> f32 {
    0.3 * c[0] + 0.59 * c[1] + 0.11 * c[2]
}

/// The saturation of a color: max minus min.
#[must_use]
fn saturation(c: [f32; 3]) -> f32 {
    c[0].max(c[1]).max(c[2]) - c[0].min(c[1]).min(c[2])
}

/// The middle value of a three-element color.
#[must_use]
fn median(c: [f32; 3]) -> f32 {
    let mut sorted = c;
    sorted.sort_by(f32::total_cmp);
    sorted[1]
}

/// Clip a color back into the unit cube, preserving luminance.
///
/// A luminosity outside `0.0..=1.0` has no in-gamut color, so the input becomes
/// its grey. This keeps the result stable and finite.
#[must_use]
fn clip_color(mut c: [f32; 3]) -> [f32; 3] {
    let l = luminance(c);
    let c_min = c[0].min(c[1]).min(c[2]);
    let c_max = c[0].max(c[1]).max(c[2]);
    if c_min < 0.0 {
        let d = l - c_min;
        for channel in &mut c {
            *channel = if l > 0.0 && d > 0.0 {
                l + (*channel - l) * l / d
            } else {
                l
            };
        }
    }
    if c_max > 1.0 {
        let d = c_max - l;
        for channel in &mut c {
            *channel = if l < 1.0 && d > 0.0 {
                l + (*channel - l) * (1.0 - l) / d
            } else {
                l
            };
        }
    }
    for channel in &mut c {
        *channel = clamp01(*channel);
    }
    c
}

/// Set the luminance of `c` to `l`, then clip.
#[must_use]
fn set_luminance(c: [f32; 3], l: f32) -> [f32; 3] {
    let d = l - luminance(c);
    clip_color([c[0] + d, c[1] + d, c[2] + d])
}

/// Set the saturation of `c` to `s`, following the PDF `SetSat` operator.
#[must_use]
fn set_saturation(c: [f32; 3], s: f32) -> [f32; 3] {
    let high = c[0].max(c[1]).max(c[2]);
    let low = c[0].min(c[1]).min(c[2]);
    let middle = median(c);
    let active = high > low;
    let mut out = [0.0f32; 3];
    for (index, value) in c.iter().enumerate() {
        if !active || value.total_cmp(&low) == std::cmp::Ordering::Equal {
            continue;
        }
        if value.total_cmp(&middle) == std::cmp::Ordering::Equal {
            out[index] = (middle - low) * s / (high - low);
        } else if value.total_cmp(&high) == std::cmp::Ordering::Equal {
            out[index] = s;
        }
    }
    out
}

/// Blend one RGB source over one RGB backdrop with `mode`.
///
/// Non-separable modes use the standard luminance and saturation formulas.
#[must_use]
pub fn blend_rgb(mode: BlendMode, backdrop: [f32; 3], source: [f32; 3]) -> [f32; 3] {
    match mode {
        BlendMode::Hue => set_luminance(
            set_saturation(source, saturation(backdrop)),
            luminance(backdrop),
        ),
        BlendMode::Saturation => set_luminance(
            set_saturation(backdrop, saturation(source)),
            luminance(backdrop),
        ),
        BlendMode::Color => set_luminance(source, luminance(backdrop)),
        BlendMode::Luminosity => set_luminance(backdrop, luminance(source)),
        BlendMode::DarkerColor => {
            if luminance(source) < luminance(backdrop) {
                source
            } else {
                backdrop
            }
        }
        BlendMode::LighterColor => {
            if luminance(source) > luminance(backdrop) {
                source
            } else {
                backdrop
            }
        }
        _ => [
            separable(mode, backdrop[0], source[0]),
            separable(mode, backdrop[1], source[1]),
            separable(mode, backdrop[2], source[2]),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::{BlendMode, blend_rgb, luminance};

    #[test]
    fn every_blend_key_maps_to_its_mode() {
        let cases: [(&[u8; 4], BlendMode); 28] = [
            (b"pass", BlendMode::PassThrough),
            (b"norm", BlendMode::Normal),
            (b"diss", BlendMode::Dissolve),
            (b"dark", BlendMode::Darken),
            (b"mul ", BlendMode::Multiply),
            (b"idiv", BlendMode::ColorBurn),
            (b"lbrn", BlendMode::LinearBurn),
            (b"dkCl", BlendMode::DarkerColor),
            (b"lite", BlendMode::Lighten),
            (b"scrn", BlendMode::Screen),
            (b"div ", BlendMode::ColorDodge),
            (b"lddg", BlendMode::LinearDodge),
            (b"lgCl", BlendMode::LighterColor),
            (b"over", BlendMode::Overlay),
            (b"sLit", BlendMode::SoftLight),
            (b"hLit", BlendMode::HardLight),
            (b"vLit", BlendMode::VividLight),
            (b"lLit", BlendMode::LinearLight),
            (b"pLit", BlendMode::PinLight),
            (b"hMix", BlendMode::HardMix),
            (b"diff", BlendMode::Difference),
            (b"smud", BlendMode::Exclusion),
            (b"fsub", BlendMode::Subtract),
            (b"fdiv", BlendMode::Divide),
            (b"hue ", BlendMode::Hue),
            (b"sat ", BlendMode::Saturation),
            (b"colr", BlendMode::Color),
            (b"lum ", BlendMode::Luminosity),
        ];
        for (key, expected) in cases {
            assert_eq!(BlendMode::from_key(*key), expected, "{key:?}");
        }
        assert_eq!(BlendMode::from_key(*b"zzzz"), BlendMode::Normal);
    }

    #[test]
    fn a_multiply_of_known_colors_matches_the_formula() {
        let result = blend_rgb(BlendMode::Multiply, [0.5, 0.25, 1.0], [0.4, 0.8, 0.5]);
        assert!((result[0] - 0.2).abs() < 1e-6);
        assert!((result[1] - 0.2).abs() < 1e-6);
        assert!((result[2] - 0.5).abs() < 1e-6);
    }

    #[test]
    fn a_screen_of_known_colors_matches_the_formula() {
        let result = blend_rgb(BlendMode::Screen, [0.5, 0.25, 0.0], [0.5, 0.5, 1.0]);
        assert!((result[0] - 0.75).abs() < 1e-6);
        assert!((result[1] - 0.625).abs() < 1e-6);
        assert!((result[2] - 1.0).abs() < 1e-6);
    }

    #[test]
    fn a_hard_light_of_known_colors_matches_the_formula() {
        // cs > 0.5 uses screen(cb, 2*cs - 1), else multiply(cb, 2*cs).
        let result = blend_rgb(BlendMode::HardLight, [0.5, 0.5, 0.5], [0.75, 0.25, 0.5]);
        assert!((result[0] - 0.75).abs() < 1e-6);
        assert!((result[1] - 0.25).abs() < 1e-6);
        assert!((result[2] - 0.5).abs() < 1e-6);
    }

    #[test]
    fn vivid_light_lets_a_source_extreme_win() {
        // A black source is black over any backdrop; a white source is white. A naive
        // `color_burn(cb, 2*cs)` would make the first case 1 (its near-extreme rule),
        // so these discriminate the Vivid Light variant.
        assert!((blend_rgb(BlendMode::VividLight, [1.0; 3], [0.0; 3])[0]).abs() < 1e-6);
        assert!((blend_rgb(BlendMode::VividLight, [0.0; 3], [1.0; 3])[0] - 1.0).abs() < 1e-6);
        // Interior values follow color burn below 0.5 and color dodge above it.
        assert!((blend_rgb(BlendMode::VividLight, [0.75; 3], [0.25; 3])[0] - 0.5).abs() < 1e-6);
        assert!((blend_rgb(BlendMode::VividLight, [0.25; 3], [0.75; 3])[0] - 0.5).abs() < 1e-6);
    }

    #[test]
    fn hard_mix_thresholds_the_generic_vivid_light() {
        // Photoshop corner cases: a black backdrop under a white source is black.
        assert!((blend_rgb(BlendMode::HardMix, [0.0; 3], [1.0; 3])[0]).abs() < 1e-6);
        assert!((blend_rgb(BlendMode::HardMix, [1.0; 3], [0.0; 3])[0] - 1.0).abs() < 1e-6);
        // Interior values keep the familiar cb + cs >= 1 rule.
        assert!((blend_rgb(BlendMode::HardMix, [0.6; 3], [0.4; 3])[0] - 1.0).abs() < 1e-6);
        assert!((blend_rgb(BlendMode::HardMix, [0.6; 3], [0.3; 3])[0]).abs() < 1e-6);
        // The fixed variant: a near-white backdrop stays 1 under a black source and a
        // near-black backdrop stays 0 under a white source. The generic cb + cs rule
        // would flip both, so these discriminate the variant.
        assert!((blend_rgb(BlendMode::HardMix, [1.0 - 1e-5; 3], [0.0; 3])[0] - 1.0).abs() < 1e-6);
        assert!((blend_rgb(BlendMode::HardMix, [1e-5; 3], [1.0; 3])[0]).abs() < 1e-6);
    }

    #[test]
    fn separable_modes_match_known_values() {
        // Darken and Lighten pick per channel.
        assert!((blend_rgb(BlendMode::Darken, [0.3; 3], [0.7; 3])[0] - 0.3).abs() < 1e-6);
        assert!((blend_rgb(BlendMode::Lighten, [0.3; 3], [0.7; 3])[0] - 0.7).abs() < 1e-6);
        // Difference is the absolute channel gap.
        assert!((blend_rgb(BlendMode::Difference, [0.6; 3], [0.4; 3])[0] - 0.2).abs() < 1e-6);
        assert!((blend_rgb(BlendMode::Difference, [0.1; 3], [0.9; 3])[0] - 0.8).abs() < 1e-6);
        // Exclusion: cb + cs - 2 cb cs.
        assert!((blend_rgb(BlendMode::Exclusion, [0.6; 3], [0.3; 3])[0] - 0.54).abs() < 1e-6);
        assert!((blend_rgb(BlendMode::Exclusion, [0.5; 3], [0.5; 3])[0] - 0.5).abs() < 1e-6);
        // Subtract clamps at zero.
        assert!((blend_rgb(BlendMode::Subtract, [0.6; 3], [0.4; 3])[0] - 0.2).abs() < 1e-6);
        assert!((blend_rgb(BlendMode::Subtract, [0.3; 3], [0.5; 3])[0]).abs() < 1e-6);
        // Divide is cb / cs clamped at one, with the zero guards.
        assert!((blend_rgb(BlendMode::Divide, [0.25; 3], [0.5; 3])[0] - 0.5).abs() < 1e-6);
        assert!((blend_rgb(BlendMode::Divide, [0.0; 3], [0.0; 3])[0]).abs() < 1e-6);
        assert!((blend_rgb(BlendMode::Divide, [0.3; 3], [0.0; 3])[0] - 1.0).abs() < 1e-6);
        // Pin Light switches on the source half.
        assert!((blend_rgb(BlendMode::PinLight, [0.8; 3], [0.2; 3])[0] - 0.4).abs() < 1e-6);
        assert!((blend_rgb(BlendMode::PinLight, [0.1; 3], [0.9; 3])[0] - 0.8).abs() < 1e-6);
        // Linear Light is cb + 2 cs - 1, clamped.
        assert!((blend_rgb(BlendMode::LinearLight, [0.6; 3], [0.4; 3])[0] - 0.4).abs() < 1e-6);
        assert!((blend_rgb(BlendMode::LinearLight, [0.3; 3], [0.3; 3])[0]).abs() < 1e-6);
        assert!((blend_rgb(BlendMode::LinearLight, [0.7; 3], [0.8; 3])[0] - 1.0).abs() < 1e-6);
        // Soft Light matches Photoshop's variant in both halves; 50% gray is neutral.
        assert!((blend_rgb(BlendMode::SoftLight, [0.5; 3], [0.25; 3])[0] - 0.375).abs() < 1e-6);
        assert!((blend_rgb(BlendMode::SoftLight, [0.25; 3], [0.75; 3])[0] - 0.375).abs() < 1e-6);
        assert!((blend_rgb(BlendMode::SoftLight, [0.3; 3], [0.5; 3])[0] - 0.3).abs() < 1e-6);
    }

    #[test]
    fn burn_and_dodge_treat_near_extremes_as_exact() {
        assert!((blend_rgb(BlendMode::ColorBurn, [0.5; 3], [0.5; 3])[0]).abs() < 1e-6);
        assert!((blend_rgb(BlendMode::ColorDodge, [0.25; 3], [0.5; 3])[0] - 0.5).abs() < 1e-6);
        // A backdrop barely above black dodges to black, not to a huge value.
        assert!((blend_rgb(BlendMode::ColorDodge, [1e-6; 3], [1.0; 3])[0]).abs() < 1e-6);
    }

    #[test]
    fn non_separable_modes_move_the_expected_component() {
        let cb = [0.8, 0.2, 0.2];
        let cs = [0.1, 0.1, 0.9];
        // Luminosity takes the source luminance; Color takes the backdrop luminance.
        let lum = blend_rgb(BlendMode::Luminosity, cb, cs);
        assert!((luminance(lum) - luminance(cs)).abs() < 1e-4);
        let color = blend_rgb(BlendMode::Color, cb, cs);
        assert!((luminance(color) - luminance(cb)).abs() < 1e-4);
        // Hue and Saturation keep the backdrop luminance.
        for mode in [BlendMode::Hue, BlendMode::Saturation] {
            let out = blend_rgb(mode, cb, cs);
            assert!((luminance(out) - luminance(cb)).abs() < 1e-3, "{mode:?}");
        }
        // A grey source desaturates in Saturation mode.
        let grey = blend_rgb(BlendMode::Saturation, cb, [0.5; 3]);
        assert!((grey[0] - grey[1]).abs() < 1e-4 && (grey[1] - grey[2]).abs() < 1e-4);
    }
}
