//! Rasterize a vector mask into anti-aliased coverage.
//!
//! Each Bezier is flattened to line segments first (see `vector_flatten`).
//! Each pixel row is then cut into bands at every edge end and every edge
//! crossing, so inside a band the active edges span the full band height and
//! keep their left-to-right order.
//! Walking the edges in that order with one winding counter per subpath tells
//! whether the area between two edges is inside the combined shape. Each
//! boundary edge adds its signed trapezoid area to a row buffer, and the
//! prefix sum of that buffer is the exact covered area of every pixel.
//!
//! The fill rule is non-zero within each subpath. The subpaths fold with
//! their path operations. Pixel `(x, y)` covers the square from `(x, y)` to
//! `(x + 1, y + 1)` in viewport space.

// The grid sizes and step counts are small, and the coverage is stored as `f32`.
// Every cast below is range-checked by its inputs or rounds on purpose.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)]

use crate::decode::Viewport;
use crate::vector_flatten::flatten_subpath;
use crate::vector_mask::{PathOp, VectorMask};

/// Band heights below this are skipped.
const MIN_BAND: f64 = 1e-9;

/// One flattened subpath: a closed polygon in viewport pixels.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Component {
    /// The polygon corners. The last corner joins the first.
    pub(crate) points: Vec<(f64, f64)>,
    /// How the component combines with the ones before it.
    pub(crate) op: PathOp,
}

/// One straight edge, with `y0 < y1`.
struct Edge {
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
    /// +1 when the source runs down the page, -1 when it runs up.
    direction: i32,
    /// The index of the component that owns the edge.
    component: usize,
}

impl Edge {
    /// The x position at height `y`.
    fn x_at(&self, y: f64) -> f64 {
        self.x0 + (self.x1 - self.x0) * (y - self.y0) / (self.y1 - self.y0)
    }
}

/// The coverage of a vector mask over a viewport, row-major, `0.0..=1.0`.
///
/// The document size scales the path fractions to pixels.
#[must_use]
pub(crate) fn rasterize(
    mask: &VectorMask,
    doc_width: u32,
    doc_height: u32,
    viewport: Viewport,
) -> Vec<f32> {
    let width = usize::try_from(viewport.width()).unwrap_or(0);
    let height = usize::try_from(viewport.height()).unwrap_or(0);
    let scale = (f64::from(doc_width), f64::from(doc_height));
    let origin = (f64::from(viewport.left), f64::from(viewport.top));
    let components: Vec<Component> = mask
        .subpaths
        .iter()
        .map(|subpath| Component {
            // A subpath with no area keeps its place so later ops keep their
            // index. It has no edges, so it never hits.
            points: flatten_subpath(subpath, scale)
                .unwrap_or_default()
                .into_iter()
                .map(|(x, y)| (x - origin.0, y - origin.1))
                .collect(),
            op: subpath.op,
        })
        .collect();
    let mut coverage = coverage(&components, mask.initial_fill, width, height);
    if mask.invert {
        for value in &mut coverage {
            *value = 1.0 - *value;
        }
    }
    coverage
}

/// Whether a point is inside the folded shape, given each winding number.
fn inside(components: &[Component], winding: &[i32], initial_fill: bool) -> bool {
    let mut shape = initial_fill;
    for (index, component) in components.iter().enumerate() {
        let hit = winding[index] != 0;
        // The first subpath combines, unless the start already covers all.
        let op = if index == 0 && !initial_fill {
            PathOp::Combine
        } else {
            component.op
        };
        shape = match op {
            PathOp::Combine => shape || hit,
            PathOp::Subtract => shape && !hit,
            PathOp::Intersect => shape && hit,
            PathOp::Exclude => shape ^ hit,
        };
    }
    shape
}

/// Build the edges of all components.
fn build_edges(components: &[Component]) -> Vec<Edge> {
    let mut edges = Vec::new();
    for (component, shape) in components.iter().enumerate() {
        let count = shape.points.len();
        for index in 0..count {
            let (xa, ya) = shape.points[index];
            let (xb, yb) = shape.points[(index + 1) % count];
            #[allow(clippy::float_cmp)] // Exact: only a truly flat edge is skipped.
            if ya == yb {
                continue;
            }
            let (x0, y0, x1, y1, direction) = if ya < yb {
                (xa, ya, xb, yb, 1)
            } else {
                (xb, yb, xa, ya, -1)
            };
            edges.push(Edge {
                x0,
                y0,
                x1,
                y1,
                direction,
                component,
            });
        }
    }
    edges
}

/// Buffers that live across rows, so the row loop does not allocate.
#[derive(Default)]
struct Scratch {
    /// The band boundaries of the row.
    cuts: Vec<f64>,
    /// The x range of each active edge inside the row.
    spans: Vec<(f64, f64)>,
    /// The edges of one band: mid x, x at the top, x at the bottom, index.
    band: Vec<(f64, f64, f64, usize)>,
    /// One winding number per component.
    winding: Vec<i32>,
}

/// The coverage of a set of components over a `width` by `height` grid.
#[must_use]
pub(crate) fn coverage(
    components: &[Component],
    initial_fill: bool,
    width: usize,
    height: usize,
) -> Vec<f32> {
    let mut out = vec![0.0f32; width * height];
    if width == 0 || height == 0 {
        return out;
    }
    let edges = build_edges(components);
    let mut order: Vec<usize> = (0..edges.len()).collect();
    order.sort_by(|&a, &b| edges[a].y0.total_cmp(&edges[b].y0));
    let start_inside = inside(components, &vec![0; components.len()], initial_fill);
    let mut next = 0;
    let mut active: Vec<usize> = Vec::new();
    let mut cells = vec![0.0f64; width + 2];
    let mut scratch = Scratch::default();
    for (row, line) in out.chunks_exact_mut(width).enumerate() {
        let top = row as f64;
        let bottom = top + 1.0;
        while next < order.len() && edges[order[next]].y0 < bottom {
            active.push(order[next]);
            next += 1;
        }
        active.retain(|&index| edges[index].y1 > top);
        cells.fill(0.0);
        find_bands(&edges, &active, top, bottom, &mut scratch);
        for at in 1..scratch.cuts.len() {
            let (ya, yb) = (scratch.cuts[at - 1], scratch.cuts[at]);
            if yb - ya <= MIN_BAND {
                continue;
            }
            let band = &mut scratch.band;
            band.clear();
            for &index in &active {
                let e = &edges[index];
                if e.y0 <= ya + MIN_BAND && e.y1 >= yb - MIN_BAND {
                    let (xa, xb) = (e.x_at(ya), e.x_at(yb));
                    band.push((xa.midpoint(xb), xa, xb, index));
                }
            }
            band.sort_by(|a, b| a.0.total_cmp(&b.0));
            scratch.winding.clear();
            scratch.winding.resize(components.len(), 0);
            let mut state = start_inside;
            if state {
                cells[0] += yb - ya;
            }
            for &(_, xa, xb, index) in band.iter() {
                let edge = &edges[index];
                scratch.winding[edge.component] += edge.direction;
                let now = inside(components, &scratch.winding, initial_fill);
                if now != state {
                    let sign = if now { 1.0 } else { -1.0 };
                    add_clipped(&mut cells, xa, xb, sign * (yb - ya), width as f64);
                    state = now;
                }
            }
        }
        let mut sum = 0.0f64;
        for (value, cell) in line.iter_mut().zip(&cells) {
            sum += *cell;
            *value = sum.clamp(0.0, 1.0) as f32;
        }
    }
    out
}

/// Fill `scratch.cuts` with the band boundaries of one row: its ends, the
/// edge ends, and the crossings. Edges whose x ranges miss are not tested.
fn find_bands(edges: &[Edge], active: &[usize], top: f64, bottom: f64, scratch: &mut Scratch) {
    let Scratch { cuts, spans, .. } = scratch;
    cuts.clear();
    cuts.extend([top, bottom]);
    spans.clear();
    for &index in active {
        let e = &edges[index];
        for y in [e.y0, e.y1] {
            if y > top && y < bottom {
                cuts.push(y);
            }
        }
        let (low, high) = (top.max(e.y0), bottom.min(e.y1));
        let (xa, xb) = (e.x_at(low), e.x_at(high));
        spans.push((xa.min(xb), xa.max(xb)));
    }
    for (position, &first) in active.iter().enumerate() {
        for (offset, &second) in active[position + 1..].iter().enumerate() {
            let (a, b) = (spans[position], spans[position + 1 + offset]);
            if a.1 < b.0 || b.1 < a.0 {
                continue;
            }
            if let Some(y) = crossing(&edges[first], &edges[second], top, bottom) {
                cuts.push(y);
            }
        }
    }
    cuts.sort_by(f64::total_cmp);
}

/// The height where two edges cross inside the open range, if they do.
fn crossing(a: &Edge, b: &Edge, top: f64, bottom: f64) -> Option<f64> {
    let low = top.max(a.y0).max(b.y0);
    let high = bottom.min(a.y1).min(b.y1);
    if high - low <= MIN_BAND {
        return None;
    }
    let before = a.x_at(low) - b.x_at(low);
    let after = a.x_at(high) - b.x_at(high);
    if before * after >= 0.0 {
        return None;
    }
    let y = low + (high - low) * before / (before - after);
    (y > low && y < high).then_some(y)
}

/// Add an edge piece to the row cells, clipped to `0..=width`.
///
/// Parts left of the grid fold onto its first column. Parts right of it drop
/// into a spare cell. `area` is the signed band height.
fn add_clipped(cells: &mut [f64], xa: f64, xb: f64, area: f64, width: f64) {
    let (low, high) = (xa.min(xb), xa.max(xb));
    if high <= 0.0 {
        add_piece(cells, 0.0, 0.0, area);
    } else if low >= width {
        add_piece(cells, width, width, area);
    } else if low >= 0.0 && high <= width {
        add_piece(cells, xa, xb, area);
    } else {
        // Height is linear in x along the edge, so shares follow x spans.
        let span = high - low;
        let left = (-low).max(0.0);
        let right = (high - width).max(0.0);
        if left > 0.0 {
            add_piece(cells, 0.0, 0.0, area * left / span);
        }
        if right > 0.0 {
            add_piece(cells, width, width, area * right / span);
        }
        let inner = span - left - right;
        if inner > 0.0 {
            add_piece(cells, low.max(0.0), high.min(width), area * inner / span);
        }
    }
}

/// Add one edge piece inside the grid. The prefix sum of the cells is the
/// covered area to the right of the piece.
fn add_piece(cells: &mut [f64], xa: f64, xb: f64, area: f64) {
    let (low, high) = (xa.min(xb), xa.max(xb));
    let low_floor = low.floor();
    let high_ceil = high.ceil();
    let low_index = low_floor as usize;
    let high_index = high_ceil as usize;
    if high_index <= low_index + 1 {
        let mid = low.midpoint(high) - low_floor;
        cells[low_index] += area - area * mid;
        cells[low_index + 1] += area * mid;
        return;
    }
    let inverse = 1.0 / (high - low);
    let low_frac = low - low_floor;
    let first = 0.5 * inverse * (1.0 - low_frac) * (1.0 - low_frac);
    let high_frac = high - high_ceil + 1.0;
    let last = 0.5 * inverse * high_frac * high_frac;
    cells[low_index] += area * first;
    if high_index == low_index + 2 {
        cells[low_index + 1] += area * (1.0 - first - last);
    } else {
        let second = inverse * (1.5 - low_frac);
        cells[low_index + 1] += area * (second - first);
        for cell in &mut cells[low_index + 2..high_index - 1] {
            *cell += area * inverse;
        }
        let tail = second + (high_index - low_index - 3) as f64 * inverse;
        cells[high_index - 1] += area * (1.0 - tail - last);
    }
    cells[high_index] += area * last;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vector_mask::{Knot, Subpath};

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
        let full = rasterize(&mask, 32, 32, Viewport::full(32, 32));
        let tile = rasterize(&mask, 32, 32, Viewport::new(8, 4, 24, 20));
        for y in 0..16 {
            for x in 0..16 {
                let want = full[(y + 4) * 32 + x + 8];
                assert!((tile[y * 16 + x] - want).abs() < 1e-5, "({x}, {y})");
            }
        }
    }
}
