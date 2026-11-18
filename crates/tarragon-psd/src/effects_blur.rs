//! The separable box blur for coverage planes.

use crate::effects_map::Coverage;

/// Blur a plane with two separable box passes.
///
/// The box supports a fractional width, so a size below two pixels still
/// softens rather than snapping to a hard edge. A width of zero or less
/// leaves the plane unchanged.
///
/// The passes have no cancel check inside them. One call with a 250 px box on
/// an 8192x8192 plane took about 4 s in a release build, so a cancel or a
/// deadline waits at most about that long for one call. The paint code checks
/// both between effects.
pub(crate) fn blur(plane: &mut Coverage, width: f32) {
    let width = width.clamp(0.0, 2.0 * crate::effects::MAX_EFFECT_SIZE);
    if width <= 0.0 {
        return;
    }
    let mut current = plane.values.clone();
    let mut next = vec![0.0f32; current.len()];
    for _ in 0..2 {
        box_pass(&current, &mut next, plane.width, plane.height, width, true);
        box_pass(&next, &mut current, plane.width, plane.height, width, false);
    }
    plane.values = current;
}

/// Run one box pass. The horizontal pass walks rows; the vertical pass walks
/// columns.
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    reason = "the box sum is bounded by the plane area, far below the f32 exact range"
)]
fn box_pass(
    input: &[f32],
    output: &mut [f32],
    width: usize,
    height: usize,
    box_width: f32,
    horizontal: bool,
) {
    let (lines, line_len) = if horizontal {
        (height, width)
    } else {
        (width, height)
    };
    let half = f64::from(box_width.max(0.0)) / 2.0;
    let mut prefix = vec![0.0f64; line_len + 1];
    let mut line = vec![0.0f32; line_len];
    for line_index in 0..lines {
        for (position, slot) in line.iter_mut().enumerate() {
            *slot = if horizontal {
                input[line_index * width + position]
            } else {
                input[position * width + line_index]
            };
        }
        prefix[0] = 0.0;
        for position in 0..line_len {
            prefix[position + 1] = prefix[position] + f64::from(line[position]);
        }
        for position in 0..line_len {
            let centre = position as f64 + 0.5;
            let low = (centre - half).max(0.0);
            let high = (centre + half).min(line_len as f64);
            // Outside the plane is zero, so the divisor stays the full box
            // width. A shorter divisor at an edge would add mass there.
            let span = f64::from(box_width.max(1e-6));
            let value = if span <= 1e-6 {
                f64::from(line[position])
            } else {
                (fraction(&prefix, high) - fraction(&prefix, low)) / span
            };
            let value = value.clamp(0.0, 1.0) as f32;
            if horizontal {
                output[line_index * width + position] = value;
            } else {
                output[position * width + line_index] = value;
            }
        }
    }
}

/// Linear interpolation into a prefix sum table.
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the prefix table is small and the position is clamped to its bounds"
)]
fn fraction(prefix: &[f64], position: f64) -> f64 {
    let position = position.clamp(0.0, (prefix.len() - 1) as f64);
    let low = position.floor() as usize;
    let high = (low + 1).min(prefix.len() - 1);
    prefix[low] + (prefix[high] - prefix[low]) * (position - low as f64)
}
