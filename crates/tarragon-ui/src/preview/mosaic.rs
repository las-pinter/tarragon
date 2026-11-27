//! The mosaic that replaces the picture box for a multi-selection.
//!
//! The cells fill the box: the column count follows the number of pictures,
//! the rows share the height, and when the last row would not be full the top
//! rows start with a cell two columns wide, so no slot stays empty. Each
//! picture is fitted whole inside its cell.

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

/// How many columns `count` cells take: one for two, two for three to six,
/// three for seven to nine, and the ceiling of the square root above that.
fn columns(count: usize) -> usize {
    match count {
        0..=2 => 1,
        3..=6 => 2,
        7..=9 => 3,
        _ => (3..count).find(|c| c * c >= count).unwrap_or(count),
    }
}

/// How many images the mosaic draws out of `count`, given the `cap`.
fn shown(count: usize, cap: usize) -> usize {
    count.min(cap)
}

/// The place of each of `count` cells: its row, its first column, and how
/// many columns it spans.
///
/// The rows are as few as the columns allow. Each slot the cells do not fill
/// goes to a cell two columns wide at the start of a row, from the top row
/// down, so every row is full.
fn places(count: usize) -> Vec<(usize, usize, usize)> {
    if count == 0 {
        return Vec::new();
    }
    let cols = columns(count);
    let rows = count.div_ceil(cols);
    let mut wide = rows * cols - count;
    let mut placed = Vec::with_capacity(count);
    for row in 0..rows {
        let mut col = 0;
        while col < cols && placed.len() < count {
            let span = if col == 0 && wide > 0 && cols > 1 {
                wide -= 1;
                2
            } else {
                1
            };
            placed.push((row, col, span));
            col += span;
        }
    }
    placed
}

/// The cell rects for the first `cap` of `count` pictures, filling `area`.
/// An area too small for the gaps gives no cells.
#[must_use]
#[expect(
    clippy::cast_precision_loss,
    reason = "a cell count is tiny, far below 2^24"
)]
pub fn grid_cells(count: usize, cap: usize, area: Rect) -> Vec<Rect> {
    let n = shown(count, cap);
    let places = places(n);
    let Some(&(last_row, ..)) = places.last() else {
        return Vec::new();
    };
    let (cols, rows) = (columns(n) as f32, (last_row + 1) as f32);
    let width = (area.width() - GAP * (cols - 1.0)) / cols;
    let height = (area.height() - GAP * (rows - 1.0)) / rows;
    if width <= 0.0 || height <= 0.0 {
        return Vec::new();
    }
    places
        .into_iter()
        .map(|(row, col, span)| {
            let x = area.min.x + col as f32 * (width + GAP);
            let y = area.min.y + row as f32 * (height + GAP);
            let w = span as f32 * width + (span as f32 - 1.0) * GAP;
            Rect::from_min_size(pos2(x, y), vec2(w, height))
        })
        .collect()
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
