//! Flatten vector mask subpaths into polygons.
//!
//! Each cubic Bezier becomes line segments. The work is done in document
//! pixels, so the step counts never depend on the tile that is rendered.

// The step count is small and clamped. The casts round on purpose.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)]

use crate::vector_mask::{Knot, Subpath};

/// The flatness tolerance of the Bezier flattening, in pixels.
const FLATNESS: f64 = 0.1;
/// The most line segments one Bezier flattens to.
const MAX_SEGMENTS: usize = 512;

/// Flatten a subpath to polygon corners in document pixels.
///
/// `scale` is the document width and height. An open subpath closes with a
/// straight line. The result is `None` when the subpath has fewer than two
/// knots or a point is not finite.
#[must_use]
pub(crate) fn flatten_subpath(subpath: &Subpath, scale: (f64, f64)) -> Option<Vec<(f64, f64)>> {
    let knots = &subpath.knots;
    if knots.len() < 2 {
        return None;
    }
    let map = |point: (f64, f64)| (point.0 * scale.0, point.1 * scale.1);
    let mut points = vec![map(knots[0].anchor)];
    let segments = if subpath.closed {
        knots.len()
    } else {
        knots.len() - 1
    };
    for index in 0..segments {
        let from: &Knot = &knots[index];
        let to: &Knot = &knots[(index + 1) % knots.len()];
        flatten_cubic(
            [
                map(from.anchor),
                map(from.leaving),
                map(to.preceding),
                map(to.anchor),
            ],
            &mut points,
        );
    }
    // A closed path ends on its first anchor. The polygon closes by itself.
    if subpath.closed {
        points.pop();
    }
    if points.iter().any(|p| !p.0.is_finite() || !p.1.is_finite()) {
        return None;
    }
    Some(points)
}

/// True when both controls lie on the chord, between its ends.
fn is_straight(p: &[(f64, f64); 4]) -> bool {
    let (dx, dy) = (p[3].0 - p[0].0, p[3].1 - p[0].1);
    let length_sq = dx * dx + dy * dy;
    if length_sq <= f64::EPSILON {
        return p
            .iter()
            .all(|q| (q.0 - p[0].0).abs() + (q.1 - p[0].1).abs() < 1e-9);
    }
    p[1..3].iter().all(|q| {
        let (ex, ey) = (q.0 - p[0].0, q.1 - p[0].1);
        let along = (ex * dx + ey * dy) / length_sq;
        let across = (ex * dy - ey * dx).abs() / length_sq.sqrt();
        across < 1e-9 && (0.0..=1.0).contains(&along)
    })
}

/// Append the line segments of a cubic, without its start point.
fn flatten_cubic(p: [(f64, f64); 4], out: &mut Vec<(f64, f64)>) {
    let second = |a: (f64, f64), b: (f64, f64), c: (f64, f64)| {
        let dx = a.0 - 2.0 * b.0 + c.0;
        let dy = a.1 - 2.0 * b.1 + c.1;
        dx.hypot(dy)
    };
    if is_straight(&p) {
        out.push(p[3]);
        return;
    }
    let bend = second(p[0], p[1], p[2]).max(second(p[1], p[2], p[3]));
    // The error of n equal steps is at most 0.75 * bend / n^2.
    let steps = (0.75 * bend / FLATNESS).sqrt().ceil();
    let steps = if steps.is_finite() {
        (steps as usize).clamp(1, MAX_SEGMENTS)
    } else {
        1
    };
    for step in 1..=steps {
        let t = step as f64 / steps as f64;
        let u = 1.0 - t;
        let weight = [u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t];
        let mix = |axis: fn(&(f64, f64)) -> f64| {
            weight
                .iter()
                .zip(&p)
                .map(|(w, point)| w * axis(point))
                .sum::<f64>()
        };
        out.push((mix(|point| point.0), mix(|point| point.1)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vector_mask::PathOp;

    /// A knot with all three points on one spot.
    fn corner(x: f64, y: f64) -> Knot {
        Knot {
            preceding: (x, y),
            anchor: (x, y),
            leaving: (x, y),
        }
    }

    #[test]
    fn a_straight_closed_path_keeps_its_corners() {
        let subpath = Subpath {
            knots: vec![corner(0.0, 0.0), corner(1.0, 0.0), corner(1.0, 1.0)],
            closed: true,
            op: PathOp::Combine,
        };
        let points = flatten_subpath(&subpath, (10.0, 20.0)).expect("flattens");
        assert_eq!(points, vec![(0.0, 0.0), (10.0, 0.0), (10.0, 20.0)]);
    }

    #[test]
    fn a_curve_makes_more_than_one_step() {
        let mut round = corner(1.0, 0.0);
        round.preceding = (0.5, 0.5);
        let subpath = Subpath {
            knots: vec![corner(0.0, 0.0), round],
            closed: false,
            op: PathOp::Combine,
        };
        let points = flatten_subpath(&subpath, (100.0, 100.0)).expect("flattens");
        assert!(points.len() > 2);
    }

    #[test]
    fn a_cubic_with_controls_on_the_chord_is_one_segment() {
        let mut out = Vec::new();
        flatten_cubic([(0.0, 0.0), (3.0, 0.0), (7.0, 0.0), (10.0, 0.0)], &mut out);
        assert_eq!(out, vec![(10.0, 0.0)]);
    }

    #[test]
    fn a_control_beyond_the_chord_end_is_more_than_one_segment() {
        let mut out = Vec::new();
        flatten_cubic([(0.0, 0.0), (15.0, 0.0), (3.0, 0.0), (10.0, 0.0)], &mut out);
        assert!(out.len() > 1);
    }

    #[test]
    fn swapped_controls_on_the_chord_are_still_one_segment() {
        let mut out = Vec::new();
        flatten_cubic([(0.0, 0.0), (7.0, 0.0), (3.0, 0.0), (10.0, 0.0)], &mut out);
        assert_eq!(out, vec![(10.0, 0.0)]);
    }

    #[test]
    fn a_short_or_non_finite_subpath_gives_none() {
        let mut subpath = Subpath {
            knots: vec![corner(0.0, 0.0)],
            closed: true,
            op: PathOp::Combine,
        };
        assert!(flatten_subpath(&subpath, (1.0, 1.0)).is_none());
        subpath.knots.push(corner(f64::NAN, 0.0));
        assert!(flatten_subpath(&subpath, (1.0, 1.0)).is_none());
    }
}
