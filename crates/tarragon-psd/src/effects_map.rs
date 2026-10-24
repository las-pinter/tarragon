//! Coverage maps for layer effects.
//!
//! Every effect works on a single-channel coverage plane built from the
//! layer's shape. The plane is the padded viewport, so a blur or a distance
//! field sees the pixels just outside a tile and no seam appears. The plane
//! math here is plain `f32`; each step is deterministic and independent of
//! the tile order.

use crate::effects::{Contour, GlobalLight, Glow, GlowSource, Satin, Shadow, StrokePosition};
use crate::effects_blur::blur;

/// The straight-line weight of a diagonal chamfer step.
const DIAGONAL: f32 = std::f32::consts::SQRT_2;

/// A single-channel coverage plane.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Coverage {
    /// The plane width in pixels.
    pub(crate) width: usize,
    /// The plane height in pixels.
    pub(crate) height: usize,
    /// The coverage values, row-major, in `0.0..=1.0`.
    pub(crate) values: Vec<f32>,
}

impl Coverage {
    /// A plane filled with one value.
    pub(crate) fn new(width: usize, height: usize, fill: f32) -> Self {
        Self {
            width,
            height,
            values: vec![fill; width * height],
        }
    }

    /// The value at a pixel. A pixel outside the plane returns `outside`.
    pub(crate) fn get(&self, x: i64, y: i64, outside: f32) -> f32 {
        if x < 0 || y < 0 {
            return outside;
        }
        let (Ok(x), Ok(y)) = (usize::try_from(x), usize::try_from(y)) else {
            return outside;
        };
        if x >= self.width || y >= self.height {
            return outside;
        }
        self.values[y * self.width + x]
    }

    /// A copy shifted by whole pixels. `outside` fills the vacated edge.
    #[allow(
        clippy::cast_possible_wrap,
        reason = "plane coordinates are far below the i64 range"
    )]
    pub(crate) fn shifted(&self, dx: i32, dy: i32, outside: f32) -> Self {
        let mut out = Self::new(self.width, self.height, outside);
        for y in 0..self.height {
            for x in 0..self.width {
                out.values[y * self.width + x] =
                    self.get(x as i64 - i64::from(dx), y as i64 - i64::from(dy), outside);
            }
        }
        out
    }

    /// A copy with an inverted coverage.
    pub(crate) fn inverted(&self) -> Self {
        Self {
            width: self.width,
            height: self.height,
            values: self.values.iter().map(|v| 1.0 - v).collect(),
        }
    }

    /// A copy with a per-value transform.
    pub(crate) fn map(&self, f: impl Fn(f32) -> f32) -> Self {
        Self {
            width: self.width,
            height: self.height,
            values: self.values.iter().map(|v| f(*v)).collect(),
        }
    }

    /// Multiply every value by another plane of the same size.
    pub(crate) fn multiply(&mut self, other: &Self) {
        for (value, factor) in self.values.iter_mut().zip(&other.values) {
            *value *= *factor;
        }
    }
}

/// The pixel offset of a light at `angle` and `distance`.
#[must_use]
pub(crate) fn offset(angle: f32, distance: f32) -> (i32, i32) {
    let radians = angle.to_radians();
    let dx = (-radians.cos() * distance).round();
    let dy = (radians.sin() * distance).round();
    (clamp_i32(dx), clamp_i32(dy))
}

/// Convert a finite `f32` to `i32`, saturating.
#[allow(
    clippy::cast_possible_truncation,
    reason = "the value is clamped to a small pixel offset first"
)]
fn clamp_i32(value: f32) -> i32 {
    if value.is_finite() {
        value.clamp(-4096.0, 4096.0) as i32
    } else {
        0
    }
}

/// The dilation radius and blur width of a size and spread pair.
#[must_use]
pub(crate) fn spread_split(size: f32, spread: f32) -> (f32, f32) {
    let spread = spread.clamp(0.0, 1.0);
    ((size * spread).round(), size * (1.0 - spread))
}

/// Chamfer distance from every pixel to the nearest seed pixel.
///
/// The two passes use a 1 and `sqrt(2)` step. Seeds have distance zero. A
/// pixel with no seed reachable keeps the large start value.
fn chamfer(seeds: &[bool], width: usize, height: usize) -> Vec<f32> {
    let mut distance = vec![f32::MAX; width * height];
    for (index, seed) in seeds.iter().enumerate() {
        if *seed {
            distance[index] = 0.0;
        }
    }
    for y in 0..height {
        for x in 0..width {
            let index = y * width + x;
            let mut best = distance[index];
            if x > 0 {
                best = best.min(distance[index - 1] + 1.0);
            }
            if y > 0 {
                best = best.min(distance[index - width] + 1.0);
                if x > 0 {
                    best = best.min(distance[index - width - 1] + DIAGONAL);
                }
                if x + 1 < width {
                    best = best.min(distance[index - width + 1] + DIAGONAL);
                }
            }
            distance[index] = best;
        }
    }
    for y in (0..height).rev() {
        for x in (0..width).rev() {
            let index = y * width + x;
            let mut best = distance[index];
            if x + 1 < width {
                best = best.min(distance[index + 1] + 1.0);
            }
            if y + 1 < height {
                best = best.min(distance[index + width] + 1.0);
                if x + 1 < width {
                    best = best.min(distance[index + width + 1] + DIAGONAL);
                }
                if x > 0 {
                    best = best.min(distance[index + width - 1] + DIAGONAL);
                }
            }
            distance[index] = best;
        }
    }
    distance
}

/// Distance from each inside pixel to the nearest outside pixel.
///
/// An outside pixel gets zero. The edge by a half-covered pixel counts as
/// outside, so a hard shape starts its inside distance at one.
#[must_use]
pub(crate) fn distance_inside(shape: &Coverage) -> Vec<f32> {
    let seeds: Vec<bool> = shape.values.iter().map(|v| *v < 0.5).collect();
    chamfer(&seeds, shape.width, shape.height)
}

/// Distance from each outside pixel to the nearest inside pixel.
///
/// An inside pixel gets zero.
#[must_use]
pub(crate) fn distance_outside(shape: &Coverage) -> Vec<f32> {
    let seeds: Vec<bool> = shape.values.iter().map(|v| *v >= 0.5).collect();
    chamfer(&seeds, shape.width, shape.height)
}

/// Grow a shape by `radius` pixels, keeping the original soft edge.
///
/// The growth uses the outside distance, so the new edge follows the same
/// chamfer metric as the stroke bands.
pub(crate) fn dilate(shape: &Coverage, radius: f32) -> Coverage {
    if radius <= 0.0 {
        return shape.clone();
    }
    let distance = distance_outside(shape);
    let mut out = shape.clone();
    for (index, value) in out.values.iter_mut().enumerate() {
        if *value > 0.0 {
            continue;
        }
        *value = (radius + 0.5 - distance[index]).clamp(0.0, 1.0);
    }
    out
}

/// Apply a transfer contour to every value.
pub(crate) fn apply_contour(mut plane: Coverage, contour: &Contour) -> Coverage {
    if contour.points.len() >= 2 {
        plane = plane.map(|v| contour.apply(v));
    }
    plane
}

/// Build the coverage of a drop shadow or an inner shadow.
///
/// The `light` is the document global light. An effect that follows it uses
/// the document angle instead of its own angle.
#[must_use]
pub(crate) fn shadow_map(
    shape: &Coverage,
    shadow: &Shadow,
    inner: bool,
    light: GlobalLight,
) -> Coverage {
    let angle = if shadow.use_global_light {
        light.angle
    } else {
        shadow.angle
    };
    let (dx, dy) = offset(angle, shadow.distance);
    let source = if inner {
        shape.inverted()
    } else {
        shape.clone()
    };
    let mut map = source.shifted(dx, dy, if inner { 1.0 } else { 0.0 });
    let (radius, width) = spread_split(shadow.size, shadow.spread);
    map = dilate(&map, radius);
    blur(&mut map, width);
    map = apply_contour(map, &shadow.contour);
    if inner {
        map.multiply(shape);
    }
    map
}

/// Build the coverage of an outer glow or an inner glow.
#[must_use]
pub(crate) fn glow_map(shape: &Coverage, glow: &Glow, inner: bool) -> Coverage {
    let (radius, width) = spread_split(glow.size, glow.spread);
    let mut map = if inner && glow.source == GlowSource::Center {
        let mut edge = dilate(&shape.inverted(), radius);
        blur(&mut edge, width);
        edge.map(|v| 1.0 - v)
    } else {
        let source = if inner {
            shape.inverted()
        } else {
            shape.clone()
        };
        let mut map = dilate(&source, radius);
        blur(&mut map, width);
        map
    };
    // The Range maps a blurred edge of 0.5 to a stronger glow.
    let range = if glow.range.is_finite() {
        glow.range.clamp(0.01, 1.0)
    } else {
        1.0
    };
    let gain = 1.0 / (range * range);
    map = map.map(|v| (v * gain).min(1.0));
    map = apply_contour(map, &glow.contour);
    if inner {
        map.multiply(shape);
    }
    map
}

/// Build the coverage of a satin sheen.
#[must_use]
pub(crate) fn satin_map(shape: &Coverage, satin: &Satin) -> Coverage {
    let (dx, dy) = offset(satin.angle, satin.distance);
    let mut forward = shape.shifted(dx, dy, 0.0);
    let mut backward = shape.shifted(-dx, -dy, 0.0);
    blur(&mut forward, satin.size);
    blur(&mut backward, satin.size);
    let mut map = Coverage {
        width: shape.width,
        height: shape.height,
        values: forward
            .values
            .iter()
            .zip(&backward.values)
            .map(|(a, b)| (a - b).abs())
            .collect(),
    };
    if satin.invert {
        map = map.inverted();
    }
    map = apply_contour(map, &satin.contour);
    map.multiply(shape);
    map
}

/// The inside and outside pixel width of a stroke.
#[must_use]
pub(crate) fn stroke_widths(width: f32, position: StrokePosition) -> (f32, f32) {
    match position {
        StrokePosition::Inside => (width, 0.0),
        StrokePosition::Outside => (0.0, width),
        StrokePosition::Center => (width / 2.0, width / 2.0),
    }
}

/// Build the inside and outside coverage of a stroke.
///
/// The inside band is nonzero only inside the shape. The outside band is
/// nonzero only outside it. A centered stroke splits its width between the
/// two bands.
#[must_use]
pub(crate) fn stroke_bands(
    shape: &Coverage,
    width: f32,
    position: StrokePosition,
) -> (Coverage, Coverage) {
    let (inside_width, outside_width) = stroke_widths(width, position);
    let inside_distance = distance_inside(shape);
    let outside_distance = distance_outside(shape);
    let mut inside = Coverage::new(shape.width, shape.height, 0.0);
    let mut outside = Coverage::new(shape.width, shape.height, 0.0);
    for index in 0..shape.values.len() {
        let coverage = shape.values[index];
        if inside_width > 0.0 && coverage > 0.0 {
            inside.values[index] =
                (inside_width + 0.5 - inside_distance[index]).clamp(0.0, 1.0) * coverage.min(1.0);
        }
        if outside_width > 0.0 && coverage < 1.0 {
            outside.values[index] = (outside_width + 0.5 - outside_distance[index]).clamp(0.0, 1.0)
                * (1.0 - coverage.min(1.0));
        }
    }
    (inside, outside)
}

#[cfg(test)]
mod tests {
    use super::{Coverage, distance_inside, distance_outside, offset, shadow_map, stroke_bands};
    use crate::effects::{Contour, EffectCommon, GlobalLight, Shadow, StrokePosition};
    use crate::effects_blur::blur;

    /// A plane with a filled rectangle.
    fn rectangle(width: usize, height: usize, rect: (usize, usize, usize, usize)) -> Coverage {
        let mut plane = Coverage::new(width, height, 0.0);
        for y in rect.1..rect.3 {
            for x in rect.0..rect.2 {
                plane.values[y * width + x] = 1.0;
            }
        }
        plane
    }

    #[test]
    fn offsets_follow_the_light_angle() {
        let (dx, dy) = offset(120.0, 10.0);
        assert_eq!((dx, dy), (5, 9));
        assert_eq!(offset(0.0, 10.0), (-10, 0));
    }

    #[test]
    fn the_inside_distance_is_zero_outside_the_shape() {
        let plane = rectangle(9, 9, (2, 2, 7, 7));
        let inside = distance_inside(&plane);
        assert_eq!(inside[0], 0.0);
        assert!(inside[4 * 9 + 4] > 1.5);
        let outside = distance_outside(&plane);
        assert_eq!(outside[4 * 9 + 4], 0.0);
        assert!(outside[0] > 1.5);
    }

    #[test]
    fn a_blur_spreads_a_single_pixel() {
        let mut plane = Coverage::new(5, 5, 0.0);
        plane.values[12] = 1.0;
        blur(&mut plane, 3.0);
        assert!(plane.values[12] < 1.0);
        assert!(plane.values[11] > 0.0);
        let sum: f32 = plane.values.iter().sum();
        assert!((sum - 1.0).abs() < 0.05, "mass {sum}");
    }

    #[test]
    fn a_drop_shadow_moves_down_and_right() {
        let shape = rectangle(21, 21, (6, 6, 15, 15));
        let shadow = Shadow {
            common: EffectCommon::default(),
            color: [0.0, 0.0, 0.0],
            angle: 120.0,
            distance: 4.0,
            size: 0.0,
            spread: 0.0,
            use_global_light: false,
            knocks_out: true,
            contour: Contour::default(),
        };
        let map = shadow_map(&shape, &shadow, false, GlobalLight::default());
        // The shadow leaves the top-left corner and covers the bottom-right.
        assert_eq!(map.values[6 * 21 + 6], 0.0);
        assert!(map.values[17 * 21 + 16] > 0.0);
    }

    #[test]
    fn a_centered_stroke_has_bands_on_both_sides() {
        let shape = rectangle(15, 15, (5, 5, 10, 10));
        let (inside, outside) = stroke_bands(&shape, 4.0, StrokePosition::Center);
        assert!(inside.values[5 * 15 + 7] > 0.5);
        assert!(outside.values[4 * 15 + 7] > 0.5);
        assert_eq!(inside.values[0], 0.0);
        assert_eq!(outside.values[7 * 15 + 7], 0.0);
    }
}
