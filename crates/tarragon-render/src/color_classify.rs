//! Color classification: map one RGB color to a color bucket.
//!
//! The bucket rules and the neutral rules live here. The palette extraction in
//! the `color` module calls [`classify`]. This module holds no state.

use image::Rgb;
use tarragon_core::color::ColorBucket;

/// A very dark color is neutral.
const VALUE_LOW: f64 = 0.08;

/// A pastel color is pink only when its saturation is at or below this limit.
const PINK_MAX_S: f64 = 0.60;

/// A pastel color is pink only when its value is at or above this limit.
const PINK_MIN_V: f64 = 0.80;

/// Hue ranges (degrees, half-open) for the hue buckets. Together they cover
/// 0..360. Each range holds its pure hue: red 0, orange 30, yellow 60, green
/// 120, cyan 180, blue 240, violet 270, magenta 300. Pink has no row: it is a
/// pastel rule in [`classify`].
const HUE_BUCKETS: [(ColorBucket, &[(f64, f64)]); 9] = [
    (ColorBucket::Red, &[(345.0, 360.0), (0.0, 15.0)]),
    (ColorBucket::Orange, &[(15.0, 42.0)]),
    (ColorBucket::Yellow, &[(42.0, 70.0)]),
    (ColorBucket::Green, &[(70.0, 150.0)]),
    (ColorBucket::Teal, &[(150.0, 175.0)]),
    (ColorBucket::Cyan, &[(175.0, 198.0)]),
    (ColorBucket::Blue, &[(198.0, 255.0)]),
    (ColorBucket::Purple, &[(255.0, 292.0)]),
    (ColorBucket::Magenta, &[(292.0, 345.0)]),
];

/// Map a palette color to its bucket, with the neutral rules.
///
/// A color is neutral when its saturation is at or below the threshold, or
/// when its value is very low. A very bright color is neutral only through the
/// saturation limit, so a bright saturated color keeps its hue bucket.
///
/// A light, soft color with a hue from 292 up to 360 or from 0 up to 15 is
/// pink. A stronger or darker color with the same hue stays magenta or red.
pub(crate) fn classify(color: Rgb<u8>, neutral_s_threshold: f64) -> ColorBucket {
    let (hue, saturation, value) = rgb_to_hsv(color);
    if saturation <= neutral_s_threshold || value <= VALUE_LOW {
        return ColorBucket::Neutral;
    }
    if (hue >= 292.0 || hue < 15.0) && saturation <= PINK_MAX_S && value >= PINK_MIN_V {
        return ColorBucket::Pink;
    }
    hue_to_bucket(hue).unwrap_or(ColorBucket::Neutral)
}

/// Convert an 8-bit RGB color to HSV (H 0..360, S 0..1, V 0..1).
fn rgb_to_hsv(color: Rgb<u8>) -> (f64, f64, f64) {
    let red = f64::from(color[0]) / 255.0;
    let green = f64::from(color[1]) / 255.0;
    let blue = f64::from(color[2]) / 255.0;
    let max = red.max(green).max(blue);
    let min = red.min(green).min(blue);
    let delta = max - min;

    if delta <= 0.0 {
        return (0.0, 0.0, max);
    }
    // Pick the max channel by integer value to avoid a float compare.
    let base = if color[0] >= color[1] && color[0] >= color[2] {
        60.0 * ((green - blue) / delta)
    } else if color[1] >= color[2] {
        60.0 * ((blue - red) / delta) + 120.0
    } else {
        60.0 * ((red - green) / delta) + 240.0
    };
    (base.rem_euclid(360.0), delta / max, max)
}

/// Map a hue (0..360) to a chromatic bucket, or none when no range holds it.
fn hue_to_bucket(hue: f64) -> Option<ColorBucket> {
    for (bucket, ranges) in HUE_BUCKETS {
        for (min, max) in ranges {
            if *min <= hue && hue < *max {
                return Some(bucket);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use tarragon_core::color::ColorBucket;

    use image::Rgb;

    use super::{classify, hue_to_bucket};

    #[test]
    fn each_hue_edge_belongs_to_the_bucket_it_starts() {
        let cases = [
            (14.9, ColorBucket::Red),
            (15.0, ColorBucket::Orange),
            (41.9, ColorBucket::Orange),
            (42.0, ColorBucket::Yellow),
            (69.9, ColorBucket::Yellow),
            (70.0, ColorBucket::Green),
            (149.9, ColorBucket::Green),
            (150.0, ColorBucket::Teal),
            (174.9, ColorBucket::Teal),
            (175.0, ColorBucket::Cyan),
            (197.9, ColorBucket::Cyan),
            (198.0, ColorBucket::Blue),
            (254.9, ColorBucket::Blue),
            (255.0, ColorBucket::Purple),
            (291.9, ColorBucket::Purple),
            (292.0, ColorBucket::Magenta),
            (344.9, ColorBucket::Magenta),
            (345.0, ColorBucket::Red),
        ];
        for (hue, expected) in cases {
            assert_eq!(hue_to_bucket(hue), Some(expected), "hue {hue}");
        }
    }

    #[test]
    fn no_hue_row_is_pink() {
        for tenth in 0_u16..3600 {
            let hue = f64::from(tenth) / 10.0;
            assert_ne!(hue_to_bucket(hue), Some(ColorBucket::Pink), "hue {hue}");
        }
    }

    #[test]
    fn pastel_colors_with_a_red_or_magenta_hue_are_pink() {
        for color in [[255, 128, 171], [255, 160, 160], [255, 200, 200]] {
            assert_eq!(classify(Rgb(color), 0.15), ColorBucket::Pink, "{color:?}");
        }
    }

    #[test]
    fn strong_or_dark_colors_with_a_red_or_magenta_hue_are_not_pink() {
        let cases = [
            ([233, 30, 99], ColorBucket::Magenta),
            ([231, 76, 60], ColorBucket::Red),
            ([180, 90, 120], ColorBucket::Magenta),
        ];
        for (color, expected) in cases {
            assert_eq!(classify(Rgb(color), 0.15), expected, "{color:?}");
        }
    }

    #[test]
    fn a_nearly_white_pink_is_neutral() {
        assert_eq!(classify(Rgb([255, 235, 235]), 0.15), ColorBucket::Neutral);
    }

    #[test]
    fn the_pink_limits_are_inclusive() {
        // S 0.60 and V 1.0: the saturation limit itself is pink.
        assert_eq!(classify(Rgb([255, 102, 102]), 0.15), ColorBucket::Pink);
        // V 0.80 (204) with S 0.5: the value limit itself is pink.
        assert_eq!(classify(Rgb([204, 102, 102]), 0.15), ColorBucket::Pink);
        // S just above 0.60 is red.
        assert_eq!(classify(Rgb([255, 101, 101]), 0.15), ColorBucket::Red);
        // V just below 0.80 is red.
        assert_eq!(classify(Rgb([203, 102, 102]), 0.15), ColorBucket::Red);
    }

    #[test]
    fn every_hue_from_0_to_359_9_has_a_bucket() {
        for tenth in 0_u16..3600 {
            let hue = f64::from(tenth) / 10.0;
            assert!(hue_to_bucket(hue).is_some(), "no bucket for hue {hue}");
        }
    }

    #[test]
    fn red_starts_at_345() {
        assert_eq!(hue_to_bucket(345.0), Some(ColorBucket::Red));
    }
}
