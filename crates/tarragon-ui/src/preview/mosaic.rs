//! The N-up mosaic that replaces the picture box for a multi-selection.

use egui::{CornerRadius, Rect, RichText, Sense, Ui, vec2};

use super::image::{PreviewImage, paint_picture};
use crate::tokens;

/// How many images the mosaic shows when the setting is not read yet.
pub const DEFAULT_CAP: usize = 9;

/// How many columns hold `count` cells: the smallest whole number whose
/// square reaches `count`, so the grid stays close to square.
fn columns(count: usize) -> usize {
    (1..=count.max(1)).find(|c| c * c >= count).unwrap_or(1)
}

/// How many images the mosaic draws out of `count`, given the `cap`.
fn shown(count: usize, cap: usize) -> usize {
    count.min(cap)
}

/// The height of the grid for `count` images, padding included, so the caller
/// can reserve it.
#[expect(
    clippy::cast_precision_loss,
    reason = "a row count is tiny, far below 2^24"
)]
fn grid_height(count: usize, cap: usize) -> f32 {
    let n = shown(count, cap);
    if n == 0 {
        return 0.0;
    }
    let rows = n.div_ceil(columns(n)) as f32;
    rows * tokens::PREVIEW_MOSAIC_CELL_HEIGHT
        + (rows - 1.0) * tokens::SPACING_S
        + 2.0 * tokens::PREVIEW_MOSAIC_PADDING
}

/// The cell rects for the first `cap` of `count` images, row by row. The
/// columns share the width of `area` less the grid padding and each cell has
/// the fixed cell height. An area with no room gives no cells.
#[must_use]
#[expect(
    clippy::cast_precision_loss,
    reason = "a column or row index is tiny, far below 2^24"
)]
pub fn grid_cells(count: usize, cap: usize, area: Rect) -> Vec<Rect> {
    let n = shown(count, cap);
    let area = area.shrink(tokens::PREVIEW_MOSAIC_PADDING);
    if n == 0 || area.width() <= 0.0 || area.height() <= 0.0 {
        return Vec::new();
    }
    let cols = columns(n);
    let gap = tokens::SPACING_S;
    let width = ((area.width() - gap * (cols - 1) as f32) / cols as f32).max(0.0);
    let height = tokens::PREVIEW_MOSAIC_CELL_HEIGHT;
    (0..n)
        .map(|i| {
            let (col, row) = ((i % cols) as f32, (i / cols) as f32);
            let min = area.min + vec2(col * (width + gap), row * (height + gap));
            Rect::from_min_size(min, vec2(width, height))
        })
        .collect()
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
    let size = vec2(ui.available_width(), grid_height(images.len(), cap));
    let (area, _) = ui.allocate_exact_size(size, Sense::hover());
    for (cell, image) in grid_cells(images.len(), cap, area).into_iter().zip(images) {
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
                .color(tokens::TEXT_MUTED),
        );
    }
}

#[cfg(test)]
mod tests;
