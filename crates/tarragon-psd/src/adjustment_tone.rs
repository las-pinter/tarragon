//! Math for Vibrance, Black & White and Gradient Map.
//!
//! Values are normalized `f32` in `0.0..=1.0`. Block parsing lives in
//! `crate::adjustment_parse_desc`.

use crate::adjustment_color::{linear_to_srgb, srgb_to_linear};
use crate::content::sample_stops;

/// The grey weights (red, green) that Vibrance Saturation moves colors from.
/// Blue has no weight.
const VIBRANCE_GREY: [f32; 2] = [0.288, 0.712];

/// The fitted terms of the positive Vibrance saturation boost.
const VIBRANCE_BOOST: [f32; 7] = [0.5001, 0.0887, 0.4374, 0.7573, -0.8113, 0.5154, 2.8026];

/// The Rec. 601 weights of the grey that Gradient Map reads.
const MAP_GREY: [f32; 3] = [0.299, 0.587, 0.114];

/// Vibrance and Saturation on one pixel. Both sliders are fractions in
/// `-1.0..=1.0`.
///
/// Both work in linear light. Vibrance runs first, then Saturation.
#[must_use]
pub(crate) fn vibrance(rgb: [f32; 3], vibrance: f32, saturation: f32) -> [f32; 3] {
    let mut linear = rgb.map(|value| srgb_to_linear(value.clamp(0.0, 1.0)));
    if vibrance != 0.0 {
        linear = vibrance_linear(linear, vibrance);
    }
    if saturation != 0.0 {
        let grey = VIBRANCE_GREY[0] * linear[0] + VIBRANCE_GREY[1] * linear[1];
        linear = linear.map(|value| (grey + (1.0 + saturation) * (value - grey)).clamp(0.0, 1.0));
    }
    linear.map(|value| linear_to_srgb(value.clamp(0.0, 1.0)))
}

/// The Vibrance slider on a linear-light pixel.
///
/// With `t = 1 - min / max` (HSV saturation) and `V = max`, a negative slider
/// (`a = -v`) gives `t' = t (1 - a / 4) (1 - a + a t (1 + t) / 2)` and
/// `V' = V - a * shape`. A positive slider gives `V' = V + v / 4 * shape` and
/// a saturation boost that grows as `t` and `V` fall. The boost is damped on
/// reds and skin. Here `shape = t (1 - t) (2 - t + t^2) V (1 - V)`.
fn vibrance_linear(rgb: [f32; 3], slider: f32) -> [f32; 3] {
    let max = rgb[0].max(rgb[1]).max(rgb[2]);
    let min = rgb[0].min(rgb[1]).min(rgb[2]);
    if max <= 0.0 || max - min <= 1e-9 {
        return rgb;
    }
    let t = 1.0 - min / max;
    let shape = t * (1.0 - t) * (2.0 - t + t * t) * max * (1.0 - max).max(0.0);
    let (new_t, new_v) = if slider < 0.0 {
        let a = -slider;
        (
            t * (1.0 - a / 4.0) * (1.0 - a + a * t * (1.0 + t) / 2.0),
            max - a * shape,
        )
    } else {
        let [c0, c1, p, q, r0, ra, rt] = VIBRANCE_BOOST;
        let skin = skin_weight(hsv_hue(rgb, max, min));
        let a = slider * (1.0 - skin) + 0.64 * slider.powf(1.6) * skin;
        let boost = (c0 * a + c1 * a * a)
            * t.powf(p)
            * (1.0 - t).powf(q)
            * ((r0 + ra * a + rt * (1.0 - t)) * (1.0 - max).max(0.0)).exp();
        ((t * (1.0 + boost)).min(1.0), max + slider / 4.0 * shape)
    };
    let new_min = new_v * (1.0 - new_t);
    let scale = (new_v - new_min) / (max - min);
    rgb.map(|value| new_min + (value - min) * scale)
}

/// How much positive Vibrance treats a hue (degrees) as skin: 1 on 0 to 30,
/// fading to 0 by 45, and fading in again from 300.
fn skin_weight(hue: f32) -> f32 {
    if hue <= 30.0 {
        1.0
    } else if hue < 45.0 {
        (45.0 - hue) / 15.0
    } else if hue < 300.0 {
        0.0
    } else {
        (hue - 300.0) / 60.0
    }
}

/// The HSV hue in degrees. The caller gives the max and min, with `max > min`.
fn hsv_hue(rgb: [f32; 3], max: f32, min: f32) -> f32 {
    let span = max - min;
    let sector = if rgb[0] >= rgb[1] && rgb[0] >= rgb[2] {
        ((rgb[1] - rgb[2]) / span).rem_euclid(6.0)
    } else if rgb[1] >= rgb[2] {
        (rgb[2] - rgb[0]) / span + 2.0
    } else {
        (rgb[0] - rgb[1]) / span + 4.0
    };
    sector * 60.0
}

/// The grey value of Black & White.
///
/// The grey is the smallest channel, plus the secondary part (middle minus
/// smallest) times the secondary-color weight, plus the primary part (largest
/// minus middle) times the primary-color weight. `weights` are percent for
/// reds, yellows, greens, cyans, blues and magentas.
#[must_use]
fn black_white_grey(rgb: [f32; 3], weights: &[f32; 6]) -> f32 {
    let [r, g, b] = rgb;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let mid = r + g + b - max - min;
    // The primary is the largest channel; the secondary is the two largest together.
    let primary = if r >= g && r >= b {
        0
    } else if g >= b {
        2
    } else {
        4
    };
    let secondary = if b <= r && b <= g {
        1
    } else if r <= g {
        3
    } else {
        5
    };
    (min + (mid - min) * weights[secondary] / 100.0 + (max - mid) * weights[primary] / 100.0)
        .clamp(0.0, 1.0)
}

/// Black & White, with an optional tint color.
///
/// The tint rule is a best guess: the grey is multiplied by twice the tint
/// color and averaged with the plain grey.
#[must_use]
pub(crate) fn black_white(rgb: [f32; 3], weights: &[f32; 6], tint: Option<[f32; 3]>) -> [f32; 3] {
    let grey = black_white_grey(rgb, weights);
    match tint {
        Some(color) => {
            std::array::from_fn(|i| (grey * color[i] * 2.0).clamp(0.0, 1.0) * 0.5 + grey * 0.5)
        }
        None => [grey; 3],
    }
}

/// Gradient Map: the Rec. 601 grey picks a color on the ramp.
///
/// `stops` are sorted by location and `midpoints` hold one value per segment.
/// The result is opaque, so the stop alpha is ignored.
#[must_use]
pub(crate) fn gradient_map(
    rgb: [f32; 3],
    stops: &[(f32, [f32; 4])],
    midpoints: &[f32],
    reverse: bool,
) -> [f32; 3] {
    let grey = (MAP_GREY[0] * rgb[0] + MAP_GREY[1] * rgb[1] + MAP_GREY[2] * rgb[2]).clamp(0.0, 1.0);
    let t = if reverse { 1.0 - grey } else { grey };
    let color = sample_stops(stops, midpoints, t);
    [color[0], color[1], color[2]]
}

#[cfg(test)]
mod tests {
    use super::{black_white, gradient_map, vibrance};

    fn chroma(rgb: [f32; 3]) -> f32 {
        rgb[0].max(rgb[1]).max(rgb[2]) - rgb[0].min(rgb[1]).min(rgb[2])
    }

    #[test]
    fn zero_sliders_are_the_identity() {
        let pixel = [0.7, 0.4, 0.2];
        let out = vibrance(pixel, 0.0, 0.0);
        for (a, b) in pixel.iter().zip(out) {
            assert!((a - b).abs() < 1e-3, "{pixel:?} {out:?}");
        }
    }

    #[test]
    fn full_negative_saturation_is_grey_in_red_and_green() {
        let out = vibrance([0.8, 0.3, 0.2], 0.0, -1.0);
        assert!((out[0] - out[1]).abs() < 1e-4, "{out:?}");
    }

    #[test]
    fn black_and_white_weight_the_primary_and_secondary_parts() {
        let weights = [40.0, 60.0, 40.0, 60.0, 20.0, 80.0];
        let red = black_white([1.0, 0.0, 0.0], &weights, None);
        assert!((red[0] - 0.4).abs() < 1e-6 && (red[0] - red[2]).abs() < 1e-6);
        let yellow = black_white([1.0, 1.0, 0.0], &weights, None);
        assert!((yellow[0] - 0.6).abs() < 1e-6, "{yellow:?}");
    }

    #[test]
    fn each_pure_hue_gives_its_own_weight() {
        let weights = [20.0, 40.0, 60.0, 80.0, 100.0, 10.0];
        let pixels = [
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 1.0, 1.0],
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 1.0],
        ];
        for (pixel, weight) in pixels.into_iter().zip(weights) {
            let out = black_white(pixel, &weights, None);
            assert!((out[0] - weight / 100.0).abs() < 1e-6, "{pixel:?} {out:?}");
        }
    }

    #[test]
    fn a_tint_shifts_the_grey_toward_the_tint_color() {
        let weights = [40.0, 60.0, 40.0, 60.0, 20.0, 80.0];
        let out = black_white([0.5, 0.5, 0.5], &weights, Some([1.0, 0.5, 0.0]));
        assert!(out[0] > out[1] && out[1] > out[2], "{out:?}");
    }

    #[test]
    fn a_gradient_map_reads_the_ends_and_reverses() {
        let stops = [(0.0, [1.0, 0.0, 0.0, 1.0]), (1.0, [0.0, 0.0, 1.0, 1.0])];
        assert_eq!(
            gradient_map([0.0; 3], &stops, &[0.5], false),
            [1.0, 0.0, 0.0]
        );
        assert_eq!(
            gradient_map([1.0; 3], &stops, &[0.5], false),
            [0.0, 0.0, 1.0]
        );
        assert_eq!(
            gradient_map([1.0; 3], &stops, &[0.5], true),
            [1.0, 0.0, 0.0]
        );
    }

    #[test]
    fn positive_vibrance_helps_muted_colors_more_than_vivid_ones() {
        let muted = [0.45, 0.5, 0.4];
        let vivid = [0.9, 0.1, 0.1];
        let gain = |pixel: [f32; 3]| chroma(vibrance(pixel, 0.8, 0.0)) / chroma(pixel);
        assert!(gain(muted) > gain(vivid), "{} {}", gain(muted), gain(vivid));
    }
}
