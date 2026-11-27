//! The mosaic that replaces the picture box for a multi-selection.
//!
//! The cells fill the box in reading order. The rows are near square and
//! balanced, longer rows first; each row splits the full width among its
//! cells, so no slot stays empty, and the rows share the height. Each picture
//! is fitted whole inside its cell.

use std::path::PathBuf;

use egui::epaint::RectShape;
use egui::{Color32, CornerRadius, Pos2, Rect, Sense, Ui, pos2, vec2};

use super::image::{PreviewImage, fit_rect};
use crate::{grid, tokens};

/// How many images the mosaic shows when the setting is not read yet.
pub const DEFAULT_CAP: usize = 9;

/// The gap between two cells, in points.
const GAP: f32 = 8.0;

/// One ready picture of the selection and the file it shows, for the badge.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MosaicCell {
    /// The picture.
    pub image: PreviewImage,
    /// The selected file.
    pub path: PathBuf,
}

/// How many cells the widest row holds: the ceiling of the square root of
/// `count`, so the grid stays near square.
fn columns(count: usize) -> usize {
    (1..=count).find(|c| c * c >= count).unwrap_or(1)
}

/// How many images the mosaic draws out of `count`, given the `cap`.
fn shown(count: usize, cap: usize) -> usize {
    count.min(cap)
}

/// How many cells each row holds, top row first. The rows are as few as the
/// columns allow, they differ by at most one cell, and the longer rows come
/// first: three gives [2, 1], five [3, 2], and seven [3, 2, 2].
fn row_lengths(count: usize) -> Vec<usize> {
    if count == 0 {
        return Vec::new();
    }
    let rows = count.div_ceil(columns(count));
    let (base, extra) = (count / rows, count % rows);
    (0..rows)
        .map(|row| base + usize::from(row < extra))
        .collect()
}

/// The cell rects for the first `cap` of `count` pictures, filling `area`,
/// in reading order: left to right, then top to bottom. Each row splits the
/// full width among its cells, and the rows share the height. An area too
/// small for the gaps gives no cells.
#[must_use]
#[expect(
    clippy::cast_precision_loss,
    reason = "a cell count is tiny, far below 2^24"
)]
pub fn grid_cells(count: usize, cap: usize, area: Rect) -> Vec<Rect> {
    let lengths = row_lengths(shown(count, cap));
    if lengths.is_empty() {
        return Vec::new();
    }
    let rows = lengths.len() as f32;
    let height = (area.height() - GAP * (rows - 1.0)) / rows;
    let widest = columns(shown(count, cap)) as f32;
    if height <= 0.0 || area.width() - GAP * (widest - 1.0) <= 0.0 {
        return Vec::new();
    }
    let mut cells = Vec::new();
    for (row, &length) in lengths.iter().enumerate() {
        let across = length as f32;
        let width = (area.width() - GAP * (across - 1.0)) / across;
        let y = area.min.y + row as f32 * (height + GAP);
        for col in 0..length {
            let x = area.min.x + col as f32 * (width + GAP);
            cells.push(Rect::from_min_size(pos2(x, y), vec2(width, height)));
        }
    }
    cells
}

/// The note for the selected images the mosaic does not draw, or `None` when
/// it draws them all.
#[must_use]
pub fn more_hint(total: usize, shown: usize) -> Option<String> {
    (total > shown).then(|| format!("+{} more", total - shown))
}

/// How many of `cells` the mosaic draws with the `cap`.
#[must_use]
pub fn drawn(cells: &[MosaicCell], cap: usize) -> usize {
    shown(cells.len(), cap)
}

/// Draw the mosaic `height` tall across the width: each cell on the field
/// fill with its picture fitted inside and the format badge. Returns the
/// rect the mosaic takes.
pub fn show(ui: &mut Ui, cells: &[MosaicCell], cap: usize, height: f32) -> Rect {
    let (area, _) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
    let painter = ui.painter();
    for (rect, cell) in grid_cells(cells.len(), cap, area).into_iter().zip(cells) {
        let radius = CornerRadius::same(tokens::RADIUS_L);
        painter.rect_filled(rect, radius, tokens::BG_TERTIARY);
        let picture = fit_rect(cell.image.size, rect);
        let corners = grid::touching_corners(picture, rect, radius);
        let uv = Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0));
        painter.add(
            RectShape::filled(picture, corners, Color32::WHITE)
                .with_texture(cell.image.texture, uv),
        );
        grid::paint_format_badge(painter, rect, &cell.path);
    }
    area
}

#[cfg(test)]
mod tests;
