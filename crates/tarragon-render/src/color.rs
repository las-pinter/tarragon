//! Dominant-color tag computation for the color filter.
//!
//! This module is pure CPU work: it takes a decoded image and the three
//! color-tag settings, and returns bare bucket names. The `image` crate owns
//! the decode.
//!
//! The forced-square 64x64 resize is a separate resize path, distinct from
//! the aspect-preserving cache resize.
//!
//! This module always computes. It does not read the settings and it does not
//! check `color_tag_enabled`. The caller owns that gate.

#![allow(clippy::module_name_repetitions)]

use std::collections::{BTreeMap, BTreeSet};

use image::imageops::FilterType;
use image::{DynamicImage, Rgb, RgbImage};
use tarragon_core::color::ColorBucket;

use crate::color_classify::classify;

/// The forced-square edge for the palette downsample.
const PALETTE_EDGE: u32 = 64;

/// The three color-tag settings, passed in as parameters.
///
/// The caller reads these from the typed settings registry and builds this
/// struct. The registry bounds them (`palette_size` 2..32, shares 0.0..1.0).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ColorTagConfig {
    /// Median-cut palette size. The default is 8.
    pub palette_size: usize,
    /// Minimum bucket share for a tag. The default is 0.10.
    pub min_share: f64,
    /// Saturation at or below this value is neutral. The default is 0.15.
    pub neutral_s_threshold: f64,
}

impl Default for ColorTagConfig {
    fn default() -> Self {
        Self {
            palette_size: 8,
            min_share: 0.10,
            neutral_s_threshold: 0.15,
        }
    }
}

/// Return the dominant-color tags for `image`.
///
/// The algorithm downsamples the image to a 64x64 square, median-cut
/// quantizes to `config.palette_size` colors, maps each palette color to a
/// bucket, and keeps buckets at or above `config.min_share`. An empty image
/// has no tags.
#[must_use]
pub fn extract_dominant_colors(
    image: &DynamicImage,
    config: ColorTagConfig,
) -> BTreeSet<ColorBucket> {
    if image.width() == 0 || image.height() == 0 {
        return BTreeSet::new();
    }

    let small = image.resize_exact(PALETTE_EDGE, PALETTE_EDGE, FilterType::Lanczos3);
    let rgb = small.to_rgb8();
    let histogram = histogram(&rgb);
    let total: u32 = histogram.iter().map(|color| color.count).sum();
    if total == 0 {
        return BTreeSet::new();
    }

    let palette = median_cut(&histogram, config.palette_size);
    let mut bucket_counts: BTreeMap<ColorBucket, u32> = BTreeMap::new();
    for palette_box in &palette {
        let bucket = classify(palette_box.average(), config.neutral_s_threshold);
        *bucket_counts.entry(bucket).or_insert(0) += palette_box.count();
    }

    // Sum integer counts and divide once; this is more exact at the min-share
    // boundary than a float share sum.
    bucket_counts
        .into_iter()
        .filter(|(_, count)| f64::from(*count) / f64::from(total) >= config.min_share)
        .map(|(bucket, _)| bucket)
        .collect()
}

/// One distinct RGB color and how many pixels it covers.
#[derive(Debug, Clone, Copy)]
struct ColorCount {
    red: u8,
    green: u8,
    blue: u8,
    count: u32,
}

/// A median-cut box: a group of colors that share one palette entry.
#[derive(Debug, Clone)]
struct PaletteBox {
    colors: Vec<ColorCount>,
}

impl PaletteBox {
    /// The total pixel count of the box.
    fn count(&self) -> u32 {
        self.colors.iter().map(|color| color.count).sum()
    }

    /// The weighted mean color of the box, rounded per channel.
    fn average(&self) -> Rgb<u8> {
        let total = u64::from(self.count());
        if total == 0 {
            return Rgb([0, 0, 0]);
        }
        let mut red = 0u64;
        let mut green = 0u64;
        let mut blue = 0u64;
        for color in &self.colors {
            let count = u64::from(color.count);
            red += u64::from(color.red) * count;
            green += u64::from(color.green) * count;
            blue += u64::from(color.blue) * count;
        }
        let half = total / 2;
        Rgb([
            round_channel(red, total, half),
            round_channel(green, total, half),
            round_channel(blue, total, half),
        ])
    }

    /// The widest channel and its spread, as a 0, 1, or 2 channel index.
    fn widest_channel(&self) -> (usize, u8) {
        let mut min = [u8::MAX; 3];
        let mut max = [u8::MIN; 3];
        for color in &self.colors {
            let channels = [color.red, color.green, color.blue];
            for (index, value) in channels.into_iter().enumerate() {
                min[index] = min[index].min(value);
                max[index] = max[index].max(value);
            }
        }
        let mut best = 0usize;
        let mut best_range = 0u8;
        for index in 0..3 {
            let range = max[index] - min[index];
            if range > best_range {
                best_range = range;
                best = index;
            }
        }
        (best, best_range)
    }

    /// Return true when the box holds more than one color and has a spread.
    fn can_split(&self) -> bool {
        self.colors.len() > 1 && self.widest_channel().1 > 0
    }

    /// Split the box at the median of its widest channel.
    ///
    /// Both sides hold at least one color.
    fn split(mut self) -> (Self, Self) {
        let (channel, _) = self.widest_channel();
        self.colors
            .sort_by_key(|color| channel_value(*color, channel));
        let total = self.count();
        let mut running = 0u32;
        let mut split_at = self.colors.len() - 1;
        for (index, color) in self.colors.iter().enumerate() {
            running += color.count;
            if running.saturating_mul(2) >= total {
                split_at = index + 1;
                break;
            }
        }
        // Keep the left side non-empty and leave one color on the right.
        let split_at = split_at.clamp(1, self.colors.len() - 1);
        let right = self.colors.split_off(split_at);
        (
            Self {
                colors: self.colors,
            },
            Self { colors: right },
        )
    }
}

/// Divide a weighted channel sum by the pixel total with rounding.
fn round_channel(sum: u64, total: u64, half: u64) -> u8 {
    let value = (sum + half) / total;
    u8::try_from(value).unwrap_or(u8::MAX)
}

/// Return the 0, 1, or 2 channel value of a color.
fn channel_value(color: ColorCount, channel: usize) -> u8 {
    match channel {
        0 => color.red,
        1 => color.green,
        _ => color.blue,
    }
}

/// Count the distinct colors of a decoded RGB image.
fn histogram(image: &RgbImage) -> Vec<ColorCount> {
    let mut counts: BTreeMap<(u8, u8, u8), u32> = BTreeMap::new();
    for pixel in image.pixels() {
        *counts.entry((pixel[0], pixel[1], pixel[2])).or_insert(0) += 1;
    }
    counts
        .into_iter()
        .map(|((red, green, blue), count)| ColorCount {
            red,
            green,
            blue,
            count,
        })
        .collect()
}

/// Reduce the histogram to at most `palette_size` boxes by median cut.
fn median_cut(histogram: &[ColorCount], palette_size: usize) -> Vec<PaletteBox> {
    if histogram.is_empty() || palette_size == 0 {
        return Vec::new();
    }
    let mut boxes = vec![PaletteBox {
        colors: histogram.to_vec(),
    }];
    while boxes.len() < palette_size {
        let mut best: Option<usize> = None;
        for (index, palette_box) in boxes.iter().enumerate() {
            if !palette_box.can_split() {
                continue;
            }
            let better = best.is_none_or(|current| palette_box.count() > boxes[current].count());
            if better {
                best = Some(index);
            }
        }
        let Some(index) = best else { break };
        let palette_box = boxes.swap_remove(index);
        let (left, right) = palette_box.split();
        boxes.push(left);
        boxes.push(right);
    }
    boxes
}

#[cfg(test)]
mod tests {
    // Casts here build test images; the library code avoids them.
    #![allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss
    )]

    use std::collections::BTreeSet;

    use image::{DynamicImage, Rgb, RgbImage};
    use tarragon_core::color::ColorBucket;

    use super::{ColorTagConfig, extract_dominant_colors};

    /// Round a 0..1 channel to 8 bits.
    fn channel(value: f64) -> u8 {
        (value * 255.0).round() as u8
    }

    /// Convert HSV to an 8-bit RGB color.
    fn hsv_to_rgb(hue: f64, saturation: f64, value: f64) -> Rgb<u8> {
        let c = value * saturation;
        let x = c * (1.0 - ((hue / 60.0) % 2.0 - 1.0).abs());
        let m = value - c;
        let (red, green, blue) = if hue < 60.0 {
            (c, x, 0.0)
        } else if hue < 120.0 {
            (x, c, 0.0)
        } else if hue < 180.0 {
            (0.0, c, x)
        } else if hue < 240.0 {
            (0.0, x, c)
        } else if hue < 300.0 {
            (x, 0.0, c)
        } else {
            (c, 0.0, x)
        };
        Rgb([channel(red + m), channel(green + m), channel(blue + m)])
    }

    /// Build a solid 100x100 image from HSV values.
    fn solid(hue: f64, saturation: f64, value: f64) -> DynamicImage {
        DynamicImage::ImageRgb8(RgbImage::from_pixel(
            100,
            100,
            hsv_to_rgb(hue, saturation, value),
        ))
    }

    /// Build a solid 100x100 image from an 8-bit RGB color.
    fn solid_rgb(color: [u8; 3]) -> DynamicImage {
        DynamicImage::ImageRgb8(RgbImage::from_pixel(100, 100, Rgb(color)))
    }

    /// Build a horizontal split: the top `ratio` is `top`, the rest is `bottom`.
    fn split(top: Rgb<u8>, bottom: Rgb<u8>, ratio: f64) -> DynamicImage {
        let (width, height) = (100u32, 100u32);
        let split_y = (f64::from(height) * ratio) as u32;
        let mut image = RgbImage::new(width, height);
        for y in 0..height {
            let color = if y < split_y { top } else { bottom };
            for x in 0..width {
                image.put_pixel(x, y, color);
            }
        }
        DynamicImage::ImageRgb8(image)
    }

    /// Build four color quadrants in TL, TR, BL, BR order.
    fn quad(colors: [Rgb<u8>; 4]) -> DynamicImage {
        let size = 100u32;
        let half = size / 2;
        let positions = [(0, 0), (half, 0), (0, half), (half, half)];
        let mut image = RgbImage::new(size, size);
        for (color, (x0, y0)) in colors.iter().zip(positions) {
            for y in y0..y0 + half {
                for x in x0..x0 + half {
                    image.put_pixel(x, y, *color);
                }
            }
        }
        DynamicImage::ImageRgb8(image)
    }

    #[test]
    fn a_solid_red_image_yields_red() {
        let tags = extract_dominant_colors(&solid(10.0, 0.8, 0.7), ColorTagConfig::default());
        assert_eq!(tags, BTreeSet::from([ColorBucket::Red]));
    }

    #[test]
    fn a_solid_blue_image_yields_blue() {
        let tags = extract_dominant_colors(&solid(170.0, 0.8, 0.7), ColorTagConfig::default());
        assert_eq!(tags, BTreeSet::from([ColorBucket::Blue]));
    }

    #[test]
    fn a_gray_image_is_neutral() {
        let tags = extract_dominant_colors(&solid_rgb([128, 128, 128]), ColorTagConfig::default());
        assert_eq!(tags, BTreeSet::from([ColorBucket::Neutral]));
    }

    #[test]
    fn a_low_saturation_color_is_neutral() {
        let tags = extract_dominant_colors(&solid(30.0, 0.1, 0.7), ColorTagConfig::default());
        assert_eq!(tags, BTreeSet::from([ColorBucket::Neutral]));
    }

    #[test]
    fn a_very_bright_color_is_neutral() {
        let tags = extract_dominant_colors(&solid(10.0, 0.8, 0.95), ColorTagConfig::default());
        assert_eq!(tags, BTreeSet::from([ColorBucket::Neutral]));
    }

    #[test]
    fn a_very_dark_color_is_neutral() {
        let tags = extract_dominant_colors(&solid(10.0, 0.8, 0.05), ColorTagConfig::default());
        assert_eq!(tags, BTreeSet::from([ColorBucket::Neutral]));
    }

    #[test]
    fn a_hue_in_the_magenta_gap_is_neutral() {
        let tags = extract_dominant_colors(&solid(320.0, 0.8, 0.7), ColorTagConfig::default());
        assert_eq!(tags, BTreeSet::from([ColorBucket::Neutral]));
    }

    #[test]
    fn the_neutral_threshold_changes_the_bucket() {
        let image = solid(30.0, 0.2, 0.7);
        let loose = ColorTagConfig {
            neutral_s_threshold: 0.25,
            ..ColorTagConfig::default()
        };
        let strict = ColorTagConfig {
            neutral_s_threshold: 0.05,
            ..ColorTagConfig::default()
        };
        assert_eq!(
            extract_dominant_colors(&image, loose),
            BTreeSet::from([ColorBucket::Neutral])
        );
        assert_eq!(
            extract_dominant_colors(&image, strict),
            BTreeSet::from([ColorBucket::Orange])
        );
    }

    #[test]
    fn a_green_and_blue_split_yields_two_tags() {
        let image = split(hsv_to_rgb(70.0, 0.8, 0.7), hsv_to_rgb(170.0, 0.8, 0.7), 0.6);
        let tags = extract_dominant_colors(&image, ColorTagConfig::default());
        assert!(
            tags.contains(&ColorBucket::Green),
            "green is missing: {tags:?}"
        );
        assert!(
            tags.contains(&ColorBucket::Blue),
            "blue is missing: {tags:?}"
        );
        assert_eq!(tags.len(), 2, "unexpected tags: {tags:?}");
    }

    #[test]
    fn a_tiny_accent_below_the_min_share_is_dropped() {
        let image = split(hsv_to_rgb(0.0, 0.0, 0.5), hsv_to_rgb(10.0, 0.8, 0.7), 0.95);
        let tags = extract_dominant_colors(&image, ColorTagConfig::default());
        assert!(
            !tags.contains(&ColorBucket::Red),
            "red slipped in: {tags:?}"
        );
        assert!(
            tags.contains(&ColorBucket::Neutral),
            "neutral is missing: {tags:?}"
        );
    }

    /// The chromatic share is about 12%, just above the default 0.10 min
    /// share. The expected set follows the tagger rules.
    #[test]
    fn a_split_near_the_min_share_keeps_both_buckets() {
        let image = split(hsv_to_rgb(10.0, 0.8, 0.7), hsv_to_rgb(0.0, 0.0, 0.5), 0.12);
        let tags = extract_dominant_colors(&image, ColorTagConfig::default());
        assert_eq!(
            tags,
            BTreeSet::from([ColorBucket::Red, ColorBucket::Neutral])
        );
    }

    #[test]
    fn an_empty_image_returns_no_tags() {
        let image = DynamicImage::ImageRgb8(RgbImage::new(0, 0));
        assert!(extract_dominant_colors(&image, ColorTagConfig::default()).is_empty());
    }

    #[test]
    fn every_chromatic_bucket_is_reachable() {
        let cases = [
            (10.0, ColorBucket::Red),
            (24.0, ColorBucket::Orange),
            (42.0, ColorBucket::Yellow),
            (68.0, ColorBucket::Green),
            (95.0, ColorBucket::Teal),
            (123.0, ColorBucket::Cyan),
            (170.0, ColorBucket::Blue),
            (235.0, ColorBucket::Purple),
            (290.0, ColorBucket::Magenta),
        ];
        for (hue, expected) in cases {
            let tags = extract_dominant_colors(&solid(hue, 0.8, 0.7), ColorTagConfig::default());
            assert_eq!(tags, BTreeSet::from([expected]), "hue {hue}");
        }
    }

    #[test]
    fn a_larger_palette_keeps_more_buckets() {
        let image = quad([
            hsv_to_rgb(10.0, 0.8, 0.7),
            hsv_to_rgb(70.0, 0.8, 0.7),
            hsv_to_rgb(170.0, 0.8, 0.7),
            hsv_to_rgb(290.0, 0.8, 0.7),
        ]);
        let few = extract_dominant_colors(
            &image,
            ColorTagConfig {
                palette_size: 2,
                ..ColorTagConfig::default()
            },
        );
        let many = extract_dominant_colors(
            &image,
            ColorTagConfig {
                palette_size: 8,
                ..ColorTagConfig::default()
            },
        );
        assert!(few.len() <= many.len(), "few {few:?} many {many:?}");
        assert_eq!(many.len(), 4, "palette 8 did not keep four: {many:?}");
        assert!(few.len() < many.len(), "palette size changed nothing");
    }

    #[test]
    fn min_share_filters_smaller_buckets() {
        let image = split(hsv_to_rgb(70.0, 0.8, 0.7), hsv_to_rgb(170.0, 0.8, 0.7), 0.7);
        let low = extract_dominant_colors(
            &image,
            ColorTagConfig {
                min_share: 0.10,
                ..ColorTagConfig::default()
            },
        );
        assert!(low.contains(&ColorBucket::Green), "green missing: {low:?}");
        assert!(low.contains(&ColorBucket::Blue), "blue missing: {low:?}");

        let mid = extract_dominant_colors(
            &image,
            ColorTagConfig {
                min_share: 0.50,
                ..ColorTagConfig::default()
            },
        );
        assert!(mid.contains(&ColorBucket::Green), "green missing: {mid:?}");
        assert!(
            !mid.contains(&ColorBucket::Blue),
            "blue slipped in: {mid:?}"
        );

        let high = extract_dominant_colors(
            &image,
            ColorTagConfig {
                min_share: 0.80,
                ..ColorTagConfig::default()
            },
        );
        assert!(high.is_empty(), "a high min share kept tags: {high:?}");
    }

    #[test]
    fn tags_are_bare_names_without_a_prefix() {
        let tags = extract_dominant_colors(&solid(10.0, 0.8, 0.7), ColorTagConfig::default());
        assert!(tags.contains(&ColorBucket::Red));
        for tag in &tags {
            assert!(
                !tag.as_str().contains("color:"),
                "bucket name carries a prefix: {tag}"
            );
        }
    }
}
