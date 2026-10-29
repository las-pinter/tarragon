//! A colour square: a bucket colour, or a checkerboard for transparency.
//!
//! The painter knows nothing about panels, so the Preview and Filter panels
//! share it.

use egui::{Color32, CornerRadius, Painter, Rect, Response, Sense, Ui, vec2};
use tarragon_core::color::ColorBucket;

use crate::tokens;

/// The side of a square in the mockup, in points.
pub const SIZE: f32 = 24.0;

/// The side of one checkerboard cell, in points. A 24 pt square gets 4 by 4.
pub const CHECKER_CELL: f32 = 6.0;

/// Which colour a square shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Swatch {
    /// One dominant-colour bucket.
    Bucket(ColorBucket),
    /// Images with transparent pixels; drawn as a checkerboard.
    Transparent,
}

impl Swatch {
    /// The 11 buckets in the canonical order, then the transparent square.
    pub const ALL: [Self; 12] = [
        Self::Bucket(ColorBucket::Red),
        Self::Bucket(ColorBucket::Orange),
        Self::Bucket(ColorBucket::Yellow),
        Self::Bucket(ColorBucket::Green),
        Self::Bucket(ColorBucket::Teal),
        Self::Bucket(ColorBucket::Cyan),
        Self::Bucket(ColorBucket::Blue),
        Self::Bucket(ColorBucket::Purple),
        Self::Bucket(ColorBucket::Magenta),
        Self::Bucket(ColorBucket::Pink),
        Self::Bucket(ColorBucket::Neutral),
        Self::Transparent,
    ];
}

/// The theme colour of a bucket.
#[must_use]
pub const fn bucket_color(bucket: ColorBucket) -> Color32 {
    match bucket {
        ColorBucket::Red => tokens::BUCKET_RED,
        ColorBucket::Orange => tokens::BUCKET_ORANGE,
        ColorBucket::Yellow => tokens::BUCKET_YELLOW,
        ColorBucket::Green => tokens::BUCKET_GREEN,
        ColorBucket::Teal => tokens::BUCKET_TEAL,
        ColorBucket::Cyan => tokens::BUCKET_CYAN,
        ColorBucket::Blue => tokens::BUCKET_BLUE,
        ColorBucket::Purple => tokens::BUCKET_PURPLE,
        ColorBucket::Magenta => tokens::BUCKET_MAGENTA,
        ColorBucket::Pink => tokens::BUCKET_PINK,
        ColorBucket::Neutral => tokens::BUCKET_NEUTRAL,
    }
}

/// One cell of the checkerboard.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cell {
    /// Where the cell sits. Edge cells are cut to the square.
    pub rect: Rect,
    /// Whether the cell takes the dark grey. The top-left cell is light.
    pub dark: bool,
    /// Round only the corners that lie on a corner of the square.
    pub corners: CornerRadius,
}

/// Split `rect` into checkerboard cells of `cell` points, row by row.
///
/// Each cell carries the square's `radius` on the corners it shares with the
/// square, so the cells together keep the rounded outline.
#[must_use]
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    reason = "a square has a few dozen cells at most"
)]
pub fn checker_cells(rect: Rect, cell: f32, radius: u8) -> Vec<Cell> {
    if cell <= 0.0 || rect.width() <= 0.0 || rect.height() <= 0.0 {
        return Vec::new();
    }
    // The small bias keeps a side that is a whole number of cells, up to float
    // error, from gaining a sliver cell.
    let count = |len: f32| ((len / cell - 1e-3).ceil() as u32).max(1);
    let (cols, rows) = (count(rect.width()), count(rect.height()));
    // Cells start at an index times the cell size, so no error piles up. The
    // last cell ends on the square's own edge.
    let edge = |origin: f32, i: u32, n: u32, end: f32| {
        if i == n {
            end
        } else {
            origin + i as f32 * cell
        }
    };
    let mut cells = Vec::new();
    for row in 0..rows {
        for col in 0..cols {
            let min = egui::pos2(
                edge(rect.min.x, col, cols, rect.max.x),
                edge(rect.min.y, row, rows, rect.max.y),
            );
            let max = egui::pos2(
                edge(rect.min.x, col + 1, cols, rect.max.x),
                edge(rect.min.y, row + 1, rows, rect.max.y),
            );
            let (top, left) = (row == 0, col == 0);
            let (bottom, right) = (row + 1 == rows, col + 1 == cols);
            let round = |on: bool| if on { radius } else { 0 };
            cells.push(Cell {
                rect: Rect::from_min_max(min, max),
                dark: (row + col) % 2 == 1,
                corners: CornerRadius {
                    nw: round(top && left),
                    ne: round(top && right),
                    sw: round(bottom && left),
                    se: round(bottom && right),
                },
            });
        }
    }
    cells
}

/// Paint one square into `rect`. `opacity` runs from 0.0 to 1.0.
///
/// The mockup square has no border, so none is drawn.
pub fn paint(painter: &Painter, rect: Rect, swatch: Swatch, opacity: f32) {
    match swatch {
        Swatch::Bucket(bucket) => {
            painter.rect_filled(
                rect,
                CornerRadius::same(tokens::RADIUS_S),
                bucket_color(bucket).gamma_multiply(opacity),
            );
        }
        Swatch::Transparent => {
            for cell in checker_cells(rect, CHECKER_CELL, tokens::RADIUS_S) {
                let color = if cell.dark {
                    tokens::CHECKER_DARK
                } else {
                    tokens::CHECKER_LIGHT
                };
                painter.rect_filled(cell.rect, cell.corners, color.gamma_multiply(opacity));
            }
        }
    }
}

/// Lay out and paint one square of `size` points. The response senses clicks.
pub fn show(ui: &mut Ui, swatch: Swatch, size: f32, opacity: f32) -> Response {
    let (rect, response) = ui.allocate_exact_size(vec2(size, size), Sense::click());
    paint(ui.painter(), rect, swatch, opacity);
    response
}

#[cfg(test)]
mod tests;
