//! The N-up mosaic that replaces the picture box for a multi-selection.
//!
//! The pictures sit in balanced, centred rows. Each cell has the shape of its
//! picture, and each row fills the full width of the pane.

use egui::{CornerRadius, Rect, RichText, Sense, Ui, pos2, vec2};

use super::image::{PreviewImage, paint_picture};
use crate::tokens;

/// How many images the mosaic shows when the setting is not read yet.
pub const DEFAULT_CAP: usize = 9;

/// The widest and the narrowest cell shape, as width over height. A more
/// extreme picture is fitted inside a cell of this shape, so one panorama
/// cannot squeeze the rest of its row.
const ASPECT_LIMITS: (f32, f32) = (0.25, 4.0);

/// The tallest row, in points: the height of the single-picture box. A row
/// that would be taller keeps this height and is centred.
const MAX_ROW_HEIGHT: f32 = tokens::PREVIEW_IMAGE_HEIGHT;

/// How many columns a square grid of `count` cells needs: the smallest whole
/// number whose square reaches `count`. It sets the number of rows.
fn columns(count: usize) -> usize {
    (1..=count.max(1)).find(|c| c * c >= count).unwrap_or(1)
}

/// How many images the mosaic draws out of `count`, given the `cap`.
fn shown(count: usize, cap: usize) -> usize {
    count.min(cap)
}

/// How many images sit in each row, top to bottom, for `count` images.
///
/// The row count is the one of a near-square grid. The images spread over the
/// rows as evenly as possible, the fuller rows first: 5 gives 3 and 2.
fn row_sizes(count: usize) -> Vec<usize> {
    if count == 0 {
        return Vec::new();
    }
    let rows = count.div_ceil(columns(count));
    let (base, extra) = (count / rows, count % rows);
    (0..rows)
        .map(|row| base + usize::from(row < extra))
        .collect()
}

/// The cell shape for a picture of `size` pixels, width over height, within
/// `ASPECT_LIMITS`. A picture with a zero side gets a square cell.
#[expect(
    clippy::cast_precision_loss,
    reason = "a side above 2^24 pixels rounds slightly, which is invisible"
)]
fn aspect((width, height): (u32, u32)) -> f32 {
    if width == 0 || height == 0 {
        return 1.0;
    }
    (width as f32 / height as f32).clamp(ASPECT_LIMITS.0, ASPECT_LIMITS.1)
}

/// The cell rects for the first `cap` pictures of `sizes`, row by row.
///
/// Only the top-left corner and the width of `area` count: the grid starts
/// one padding inside them and is as tall as its rows. Each cell has the
/// shape of its picture, and the cells of a row share the full width, so the
/// row height follows. A row taller than `MAX_ROW_HEIGHT` keeps that height
/// and is centred. An area too narrow for the gaps of its widest row gives
/// no cells.
#[must_use]
#[expect(
    clippy::cast_precision_loss,
    reason = "a cell count is tiny, far below 2^24"
)]
pub fn grid_cells(sizes: &[(u32, u32)], cap: usize, area: Rect) -> Vec<Rect> {
    let n = shown(sizes.len(), cap);
    let pad = tokens::PREVIEW_MOSAIC_PADDING;
    let width = area.width() - 2.0 * pad;
    if n == 0 || width <= 0.0 {
        return Vec::new();
    }
    let gap = tokens::SPACING_S;
    let rows = row_sizes(n);
    let widest = rows.iter().copied().max().unwrap_or(1);
    if width <= gap * (widest - 1) as f32 {
        // A pane narrower than the gaps of the widest row has no room for pictures.
        return Vec::new();
    }
    let (mut cells, mut first, mut y) = (Vec::with_capacity(n), 0, area.min.y + pad);
    for count in rows {
        let aspects: Vec<f32> = sizes[first..first + count]
            .iter()
            .map(|s| aspect(*s))
            .collect();
        let gaps = gap * (count - 1) as f32;
        let room = width - gaps;
        let height = (room / aspects.iter().sum::<f32>()).min(MAX_ROW_HEIGHT);
        let used: f32 = aspects.iter().map(|a| a * height).sum::<f32>() + gaps;
        let mut x = area.min.x + pad + (width - used).max(0.0) / 2.0;
        for shape in aspects {
            cells.push(Rect::from_min_size(
                pos2(x, y),
                vec2(shape * height, height),
            ));
            x += shape * height + gap;
        }
        first += count;
        y += height + gap;
    }
    cells
}

/// The height the grid takes below the top of `area`, padding included, so
/// the caller can reserve it. No cells take no room.
fn grid_height(cells: &[Rect], area: Rect) -> f32 {
    cells
        .iter()
        .map(|cell| cell.max.y)
        .reduce(f32::max)
        .map_or(0.0, |bottom| {
            bottom + tokens::PREVIEW_MOSAIC_PADDING - area.min.y
        })
}

/// The hint under a mosaic that draws `shown` of `total` selected images, or
/// `None` when it draws them all.
#[must_use]
pub fn more_hint(total: usize, shown: usize) -> Option<String> {
    (total > shown).then(|| format!("+{} more", total - shown))
}

/// Draw the grid of cells, each with its picture fitted inside, then the hint.
/// `total` is the whole selection, which can be larger than `images`.
pub fn show(ui: &mut Ui, images: &[PreviewImage], total: usize, cap: usize) {
    let sizes: Vec<(u32, u32)> = images.iter().map(|image| image.size).collect();
    let probe = Rect::from_min_size(ui.cursor().min, vec2(ui.available_width(), 0.0));
    let height = grid_height(&grid_cells(&sizes, cap, probe), probe);
    let (area, _) = ui.allocate_exact_size(vec2(probe.width(), height), Sense::hover());
    // The cells are laid out again on the allocated rect, which can differ
    // from the probe when the layout moves the allocation.
    for (cell, image) in grid_cells(&sizes, cap, area).into_iter().zip(images) {
        let painter = ui.painter();
        painter.rect_filled(
            cell,
            CornerRadius::same(tokens::RADIUS_M),
            tokens::BG_SECONDARY,
        );
        paint_picture(painter, image, cell, tokens::RADIUS_M);
    }
    if let Some(hint) = more_hint(total, shown(images.len(), cap)) {
        ui.add_space(tokens::SPACING_S);
        ui.label(
            RichText::new(hint)
                .size(tokens::SIZE_CAPTION)
                .color(tokens::TEXT_SECONDARY),
        );
    }
}

#[cfg(test)]
mod tests;
