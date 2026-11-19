//! Tests for the vector mask rasterizer.

use super::*;
use crate::vector_mask::{Knot, Subpath};

/// [`coverage_with`] with no deadline.
fn coverage(components: &[Component], initial_fill: bool, width: usize, height: usize) -> Vec<f32> {
    coverage_with(
        components,
        initial_fill,
        width,
        height,
        &ParseOptions::none(),
    )
    .expect("no deadline")
}

/// A rectangle component.
fn rect(left: f64, top: f64, right: f64, bottom: f64, op: PathOp) -> Component {
    Component {
        points: vec![(left, top), (right, top), (right, bottom), (left, bottom)],
        op,
    }
}

#[test]
fn a_pixel_aligned_rectangle_covers_whole_pixels() {
    let grid = coverage(&[rect(1.0, 1.0, 3.0, 3.0, PathOp::Combine)], false, 4, 4);
    for y in 0..4 {
        for x in 0..4 {
            let want = if (1..3).contains(&x) && (1..3).contains(&y) {
                1.0
            } else {
                0.0
            };
            assert!((grid[y * 4 + x] - want).abs() < 1e-6, "({x}, {y})");
        }
    }
}

#[test]
fn a_half_pixel_edge_gives_half_coverage() {
    let grid = coverage(&[rect(1.5, 0.0, 4.0, 2.0, PathOp::Combine)], false, 4, 2);
    assert!((grid[1] - 0.5).abs() < 1e-6);
    assert!((grid[2] - 1.0).abs() < 1e-6);
    assert!(grid[0].abs() < 1e-6);
    let grid = coverage(&[rect(0.0, 0.5, 2.0, 2.0, PathOp::Combine)], false, 2, 2);
    assert!((grid[0] - 0.5).abs() < 1e-6);
    assert!((grid[2] - 1.0).abs() < 1e-6);
}

#[test]
fn a_diagonal_edge_covers_half_the_pixel() {
    let triangle = Component {
        points: vec![(0.0, 0.0), (1.0, 0.0), (0.0, 1.0)],
        op: PathOp::Combine,
    };
    let grid = coverage(&[triangle], false, 1, 1);
    assert!((grid[0] - 0.5).abs() < 1e-6);
}

#[test]
fn overlapping_subpaths_follow_the_non_zero_rule() {
    let both = [
        rect(0.0, 0.0, 3.0, 1.0, PathOp::Combine),
        rect(1.0, 0.0, 4.0, 1.0, PathOp::Combine),
    ];
    let grid = coverage(&both, false, 5, 1);
    let want = [1.0, 1.0, 1.0, 1.0, 0.0];
    for (got, want) in grid.iter().zip(want) {
        assert!((got - want).abs() < 1e-6);
    }
    // A square traced twice winds twice and still counts once.
    let corners = [(0.0, 0.0), (2.0, 0.0), (2.0, 2.0), (0.0, 2.0)];
    let twice = Component {
        points: corners.iter().chain(&corners).copied().collect(),
        op: PathOp::Combine,
    };
    let grid = coverage(&[twice], false, 2, 2);
    assert!(grid.iter().all(|v| (v - 1.0).abs() < 1e-6));
}

#[test]
fn the_path_operations_fold() {
    let outer = rect(0.0, 0.0, 4.0, 1.0, PathOp::Combine);
    let cut = |op| rect(1.0, 0.0, 3.0, 1.0, op);
    let cases = [
        (PathOp::Subtract, [1.0, 0.0, 0.0, 1.0]),
        (PathOp::Intersect, [0.0, 1.0, 1.0, 0.0]),
        (PathOp::Exclude, [1.0, 0.0, 0.0, 1.0]),
    ];
    for (op, want) in cases {
        let grid = coverage(&[outer.clone(), cut(op)], false, 4, 1);
        for (got, want) in grid.iter().zip(want) {
            assert!((got - want).abs() < 1e-6, "{op:?}");
        }
    }
}

#[test]
fn the_initial_fill_covers_everything_first() {
    let hole = rect(1.0, 0.0, 2.0, 1.0, PathOp::Subtract);
    let first = rect(0.0, 0.0, 1.0, 1.0, PathOp::Combine);
    let grid = coverage(&[first, hole], true, 3, 1);
    let want = [1.0, 0.0, 1.0];
    for (got, want) in grid.iter().zip(want) {
        assert!((got - want).abs() < 1e-6);
    }
}

#[test]
fn a_shape_wider_than_the_grid_is_clipped() {
    let grid = coverage(&[rect(-5.0, -1.0, 9.0, 3.0, PathOp::Combine)], false, 3, 2);
    assert!(grid.iter().all(|v| (v - 1.0).abs() < 1e-6));
    let grid = coverage(&[rect(1.5, 0.0, 9.0, 1.0, PathOp::Combine)], false, 3, 1);
    assert!((grid[1] - 0.5).abs() < 1e-6 && (grid[2] - 1.0).abs() < 1e-6);
}

#[test]
fn an_initial_fill_lets_a_first_subtract_cut_a_hole() {
    let hole = rect(1.0, 0.0, 2.0, 1.0, PathOp::Subtract);
    let grid = coverage(&[hole], true, 3, 1);
    let want = [1.0, 0.0, 1.0];
    for (got, want) in grid.iter().zip(want) {
        assert!((got - want).abs() < 1e-6);
    }
    // Without the fill the first subtract still combines.
    let grid = coverage(&[rect(1.0, 0.0, 2.0, 1.0, PathOp::Subtract)], false, 3, 1);
    assert!((grid[1] - 1.0).abs() < 1e-6 && grid[0].abs() < 1e-6);
}

#[test]
fn a_tile_matches_the_full_render() {
    let knot = |x: f64, y: f64| Knot {
        preceding: (x, y),
        anchor: (x, y),
        leaving: (x, y),
    };
    let mask = VectorMask {
        subpaths: vec![Subpath {
            knots: vec![knot(0.1, 0.1), knot(0.9, 0.3), knot(0.4, 0.95)],
            closed: true,
            op: PathOp::Combine,
        }],
        invert: false,
        not_linked: false,
        disabled: false,
        initial_fill: false,
    };
    let none = ParseOptions::none();
    let full = rasterize(&mask, 32, 32, Viewport::full(32, 32), &none).expect("full");
    let tile = rasterize(&mask, 32, 32, Viewport::new(8, 4, 24, 20), &none).expect("tile");
    for y in 0..16 {
        for x in 0..16 {
            let want = full[(y + 4) * 32 + x + 8];
            assert!((tile[y * 16 + x] - want).abs() < 1e-5, "({x}, {y})");
        }
    }
    let cancelled = ParseOptions::none().with_cancel(std::sync::Arc::new(
        std::sync::atomic::AtomicBool::new(true),
    ));
    assert!(matches!(
        rasterize(&mask, 32, 32, Viewport::full(32, 32), &cancelled),
        Err(PsdError::Cancelled)
    ));
}

/// The non-zero winding number of `points` at `(x, y)`.
fn winding_at(points: &[(f64, f64)], x: f64, y: f64) -> i32 {
    let mut winding = 0;
    for (index, &(xa, ya)) in points.iter().enumerate() {
        let (xb, yb) = points[(index + 1) % points.len()];
        if (ya <= y) != (yb <= y) {
            let cross = xa + (y - ya) * (xb - xa) / (yb - ya);
            if cross > x {
                winding += if yb > ya { 1 } else { -1 };
            }
        }
    }
    winding
}

#[test]
fn edges_that_cross_inside_a_row_match_a_supersampled_cover() {
    // The two long edges of this bowtie cross at (1.5, 1.5), inside a
    // pixel, and the five-point star crosses itself inside rows too.
    let star: Vec<(f64, f64)> = (0..5)
        .map(|step| {
            let angle = f64::from(step * 2) * std::f64::consts::TAU / 5.0;
            (5.0 + 4.3 * angle.sin(), 5.2 - 4.3 * angle.cos())
        })
        .collect();
    let bowtie = vec![(0.0, 0.0), (3.0, 3.0), (3.0, 0.0), (0.0, 3.0)];
    let samples = 64;
    for (points, size) in [(bowtie, 3), (star, 10)] {
        let shape = Component {
            points: points.clone(),
            op: PathOp::Combine,
        };
        let grid = coverage(&[shape], false, size, size);
        for y in 0..size {
            for x in 0..size {
                let mut hits = 0;
                for sy in 0..samples {
                    for sx in 0..samples {
                        let px = x as f64 + (f64::from(sx) + 0.5) / f64::from(samples);
                        let py = y as f64 + (f64::from(sy) + 0.5) / f64::from(samples);
                        if winding_at(&points, px, py) != 0 {
                            hits += 1;
                        }
                    }
                }
                let want = f64::from(hits) / f64::from(samples * samples);
                let got = f64::from(grid[y * size + x]);
                assert!((got - want).abs() < 0.02, "({x}, {y}): {got} vs {want}");
            }
        }
    }
}
