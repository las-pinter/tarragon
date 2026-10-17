//! Virtualized grid for the image library.
//!
//! The grid owns a fixed cell size. It builds a cell only when the cell is
//! inside the scroll viewport. An off-screen item costs no work.
//!
//! The widget is not the full grid yet.

use std::ops::Range;

use egui::{Rect, ScrollArea, Ui, Vec2, pos2, vec2};

/// Fixed geometry of one grid. All values are in points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GridMetrics {
    /// Number of columns. Zero is allowed; the grid then shows nothing.
    pub columns: usize,
    /// Width and height of one cell.
    pub cell: Vec2,
    /// Gap between two cells. The gap is not added after the last cell.
    pub spacing: f32,
}

impl GridMetrics {
    /// Distance from one cell origin to the next cell origin.
    #[must_use]
    pub fn stride(&self) -> Vec2 {
        self.cell + Vec2::splat(self.spacing)
    }

    /// Number of rows that hold `item_count` items.
    #[must_use]
    pub fn row_count(&self, item_count: usize) -> usize {
        if self.columns == 0 {
            return 0;
        }
        item_count.div_ceil(self.columns)
    }

    /// Full size of the grid content, including the scroll extent.
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn content_size(&self, item_count: usize) -> Vec2 {
        let rows = self.row_count(item_count);
        if rows == 0 {
            return Vec2::ZERO;
        }
        let stride = self.stride();
        vec2(
            (self.columns as f32 * stride.x - self.spacing).max(0.0),
            (rows as f32 * stride.y - self.spacing).max(0.0),
        )
    }

    /// Rows that touch `viewport`. The caller clamps the range to the row count.
    #[must_use]
    pub fn visible_rows(&self, viewport: Rect) -> Range<usize> {
        let stride_y = self.stride().y;
        index_floor(viewport.min.y.max(0.0), stride_y)
            ..index_ceil(viewport.max.y.max(0.0), stride_y)
    }

    /// Columns that touch `viewport`. The range stays inside the column count.
    #[must_use]
    pub fn visible_cols(&self, viewport: Rect) -> Range<usize> {
        let stride_x = self.stride().x;
        let first = index_floor(viewport.min.x.max(0.0), stride_x).min(self.columns);
        let last = index_ceil(viewport.max.x.max(0.0), stride_x).min(self.columns);
        first..last
    }

    /// Rectangle of one cell, relative to the top left of the content.
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn cell_rect(&self, row: usize, col: usize) -> Rect {
        let stride = self.stride();
        Rect::from_min_size(
            pos2(col as f32 * stride.x, row as f32 * stride.y),
            self.cell,
        )
    }
}

/// A grid over `item_count` items with a fixed cell geometry.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VirtualGrid {
    /// The fixed cell geometry.
    pub metrics: GridMetrics,
    /// Total number of items in the grid.
    pub item_count: usize,
}

impl VirtualGrid {
    /// Show the grid and paint only the visible cells.
    ///
    /// `scroll_y` is the vertical scroll position in points. The function calls
    /// `cell_ui` once for each visible cell. It returns the number of cells it
    /// built. That count proves the grid virtualizes.
    pub fn show(
        self,
        ui: &mut Ui,
        scroll_y: f32,
        mut cell_ui: impl FnMut(&mut Ui, Rect, usize),
    ) -> usize {
        let content = self.metrics.content_size(self.item_count);
        let row_count = self.metrics.row_count(self.item_count);
        let mut painted = 0usize;

        ScrollArea::vertical()
            .scroll_offset(vec2(0.0, scroll_y))
            .auto_shrink([false, false])
            .show_viewport(ui, |ui, viewport| {
                ui.set_min_size(content);
                let origin = ui.max_rect().min;
                let cols = self.metrics.visible_cols(viewport);
                for row in self.metrics.visible_rows(viewport) {
                    if row >= row_count {
                        break;
                    }
                    for col in cols.clone() {
                        let index = row * self.metrics.columns + col;
                        if index >= self.item_count {
                            break;
                        }
                        let rect = self.metrics.cell_rect(row, col).translate(origin.to_vec2());
                        cell_ui(ui, rect, index);
                        painted += 1;
                    }
                }
            });

        painted
    }
}

/// Index of the first cell row or column that touches a coordinate.
fn index_floor(coord: f32, stride: f32) -> usize {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let index = (coord / stride).floor() as usize;
    index
}

/// One past the last cell row or column that touches a coordinate.
fn index_ceil(coord: f32, stride: f32) -> usize {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let index = (coord / stride).ceil() as usize;
    index
}

#[cfg(test)]
mod tests {
    use super::*;

    fn metrics() -> GridMetrics {
        GridMetrics {
            columns: 6,
            cell: vec2(240.0, 180.0),
            spacing: 8.0,
        }
    }

    #[test]
    fn row_count_rounds_up() {
        assert_eq!(metrics().row_count(0), 0);
        assert_eq!(metrics().row_count(1), 1);
        assert_eq!(metrics().row_count(6), 1);
        assert_eq!(metrics().row_count(7), 2);
        assert_eq!(metrics().row_count(10_000), 1667);
    }

    #[test]
    fn content_size_holds_all_rows() {
        let size = metrics().content_size(10_000);
        assert_eq!(size.x, 6.0 * 248.0 - 8.0);
        assert_eq!(size.y, 1667.0 * 188.0 - 8.0);
        assert_eq!(metrics().content_size(0), Vec2::ZERO);
    }

    #[test]
    fn visible_rows_cover_the_viewport() {
        let metrics = metrics();
        let viewport = Rect::from_min_size(pos2(0.0, 0.0), vec2(1480.0, 900.0));
        assert_eq!(metrics.visible_rows(viewport), 0..5);

        let scrolled = Rect::from_min_size(pos2(0.0, 188.0), vec2(1480.0, 900.0));
        assert_eq!(metrics.visible_rows(scrolled), 1..6);
    }

    #[test]
    fn visible_cols_stay_inside_the_column_count() {
        let metrics = metrics();
        let wide = Rect::from_min_size(pos2(0.0, 0.0), vec2(5000.0, 900.0));
        assert_eq!(metrics.visible_cols(wide), 0..6);
        let narrow = Rect::from_min_size(pos2(0.0, 0.0), vec2(500.0, 900.0));
        assert_eq!(metrics.visible_cols(narrow), 0..3);
    }

    #[test]
    fn cell_rect_uses_the_stride() {
        let rect = metrics().cell_rect(2, 3);
        assert_eq!(rect.min, pos2(3.0 * 248.0, 2.0 * 188.0));
        assert_eq!(rect.size(), vec2(240.0, 180.0));
    }

    #[test]
    fn zero_columns_show_nothing() {
        let metrics = GridMetrics {
            columns: 0,
            ..metrics()
        };
        assert_eq!(metrics.row_count(10), 0);
        assert_eq!(metrics.content_size(10), Vec2::ZERO);
    }
}
