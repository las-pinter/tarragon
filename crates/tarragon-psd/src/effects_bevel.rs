//! Highlight and shadow coverage for the bevel and emboss styles.

use crate::effects::{Bevel, BevelStyle, GlobalLight};
use crate::effects_blur::blur;
use crate::effects_map::{Coverage, apply_contour, distance_inside, distance_outside};

/// Build the highlight and shadow coverage of a bevel.
///
/// The `light` is the document global light. A bevel that follows it uses the
/// document angle and altitude instead of its own values.
#[allow(
    clippy::cast_possible_wrap,
    reason = "plane coordinates are far below the i64 range"
)]
#[must_use]
pub(crate) fn bevel_maps(
    shape: &Coverage,
    bevel: &Bevel,
    light: GlobalLight,
) -> (Coverage, Coverage) {
    let inside = distance_inside(shape);
    let outside = distance_outside(shape);
    let size = bevel.size.max(1.0);
    let half = (size / 2.0).max(1.0);
    let mut height = vec![0.0f32; shape.values.len()];
    for index in 0..shape.values.len() {
        let covered = shape.values[index] >= 0.5;
        height[index] = match bevel.style {
            BevelStyle::Inner | BevelStyle::StrokeEmboss => {
                if covered {
                    1.0 - (inside[index] / size).clamp(0.0, 1.0)
                } else {
                    0.0
                }
            }
            BevelStyle::Outer => {
                if covered {
                    0.0
                } else {
                    1.0 - (outside[index] / size).clamp(0.0, 1.0)
                }
            }
            BevelStyle::Emboss => {
                if covered {
                    0.5 + 0.5 * (inside[index] / half).clamp(0.0, 1.0)
                } else {
                    0.5 - 0.5 * (outside[index] / half).clamp(0.0, 1.0)
                }
            }
            BevelStyle::PillowEmboss => {
                if covered {
                    0.5 - 0.5 * (inside[index] / half).clamp(0.0, 1.0)
                } else {
                    0.5 + 0.5 * (outside[index] / half).clamp(0.0, 1.0)
                }
            }
        };
    }
    let mut height = Coverage {
        width: shape.width,
        height: shape.height,
        values: height,
    };
    if bevel.soften > 0.0 {
        blur(&mut height, bevel.soften);
    }
    let depth = bevel.depth.max(0.0) * size * if bevel.up { 1.0 } else { -1.0 };
    let light = light.resolve(bevel.use_global_light, bevel.angle, bevel.altitude);
    let (sin, cos) = light.angle.to_radians().sin_cos();
    let altitude = light.altitude.clamp(0.0, 90.0);
    let (se, ce) = altitude.to_radians().sin_cos();
    let light = [cos * ce, -sin * ce, se];
    let mut highlight = Coverage::new(shape.width, shape.height, 0.0);
    let mut shadow = Coverage::new(shape.width, shape.height, 0.0);
    for y in 0..shape.height {
        for x in 0..shape.width {
            let index = y * shape.width + x;
            let gx = (height.get(x as i64 + 1, y as i64, 0.0)
                - height.get(x as i64 - 1, y as i64, 0.0))
                * 0.5
                * depth;
            let gy = (height.get(x as i64, y as i64 + 1, 0.0)
                - height.get(x as i64, y as i64 - 1, 0.0))
                * 0.5
                * depth;
            let normal = [-gx, -gy, 1.0];
            let length = (normal[0] * normal[0] + normal[1] * normal[1] + 1.0).sqrt();
            let lighting =
                (normal[0] * light[0] + normal[1] * light[1] + normal[2] * light[2]) / length;
            let region = match bevel.style {
                BevelStyle::Outer => 1.0 - shape.values[index].min(1.0),
                BevelStyle::Inner | BevelStyle::StrokeEmboss => shape.values[index].min(1.0),
                BevelStyle::Emboss | BevelStyle::PillowEmboss => {
                    if shape.values[index] >= 0.5 {
                        shape.values[index].min(1.0)
                    } else {
                        1.0 - shape.values[index]
                    }
                }
            } * if height.values[index] > 0.001 {
                1.0
            } else {
                0.0
            };
            let k = lighting - se;
            if k > 0.0 {
                highlight.values[index] = (k / (1.0 - se).max(1e-3)).clamp(0.0, 1.0) * region;
            } else {
                shadow.values[index] = (-k / se.max(1e-3)).clamp(0.0, 1.0) * region;
            }
        }
    }
    (
        apply_contour(highlight, &bevel.gloss),
        apply_contour(shadow, &bevel.gloss),
    )
}
