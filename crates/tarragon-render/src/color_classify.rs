//! Color classification: map one RGB color to a color bucket.
//!
//! The bucket rules and the neutral rules live here. The palette extraction in
//! the `color` module calls [`classify`]. This module holds no state.

use image::Rgb;
use tarragon_core::color::ColorBucket;

/// A very bright color is neutral.
const VALUE_HIGH: f64 = 0.92;

/// A very dark color is neutral.
const VALUE_LOW: f64 = 0.08;

/// Hue ranges for the chromatic buckets. Together they cover 0..360.
const HUE_BUCKETS: [(ColorBucket, &[(f64, f64)]); 10] = [
    (ColorBucket::Red, &[(345.0, 360.0), (0.0, 15.0)]),
    (ColorBucket::Orange, &[(15.0, 33.0)]),
    (ColorBucket::Yellow, &[(33.0, 50.0)]),
    (ColorBucket::Green, &[(50.0, 85.0)]),
    (ColorBucket::Teal, &[(85.0, 105.0)]),
    (ColorBucket::Cyan, &[(105.0, 140.0)]),
    (ColorBucket::Blue, &[(140.0, 200.0)]),
    (ColorBucket::Purple, &[(200.0, 270.0)]),
    (ColorBucket::Magenta, &[(270.0, 310.0)]),
    (ColorBucket::Pink, &[(310.0, 345.0)]),
];

/// Map a palette color to its bucket, with the neutral rules.
pub(crate) fn classify(color: Rgb<u8>, neutral_s_threshold: f64) -> ColorBucket {
    let (hue, saturation, value) = rgb_to_hsv(color);
    if saturation <= neutral_s_threshold || value >= VALUE_HIGH || value <= VALUE_LOW {
        return ColorBucket::Neutral;
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

    use super::hue_to_bucket;

    #[test]
    fn magenta_ends_just_below_310() {
        assert_eq!(hue_to_bucket(309.9), Some(ColorBucket::Magenta));
    }

    #[test]
    fn pink_starts_at_310() {
        assert_eq!(hue_to_bucket(310.0), Some(ColorBucket::Pink));
    }

    #[test]
    fn pink_ends_just_below_345() {
        assert_eq!(hue_to_bucket(344.9), Some(ColorBucket::Pink));
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
