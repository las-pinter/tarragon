//! Virtualized grid for the image library.
//!
//! The grid owns a fixed cell size. It builds a cell only when the cell is
//! inside the scroll viewport. An off-screen item costs no work.
//!
//! [`VirtualGrid::show_items`] paints a full tile and lets the scroll area
//! keep its own offset. [`VirtualGrid::show_at`] pins the offset for a
//! measurement. [`VirtualGrid::show`] reports the visible cells only, so a
//! caller can paint its own.

use std::ops::Range;

use egui::{Modifiers, Rect, ScrollArea, Ui, Vec2, pos2, vec2};

use crate::tokens;

mod cell;
mod items;
mod selection;

#[cfg(test)]
mod tests;

pub use items::{GridItem, TileError, items_in_order};
pub use selection::Selection;

/// Width of one grid track, in points. The stylesheet track is 196 px.
const TRACK_WIDTH: f32 = 196.0;
/// Height of one grid cell, in points. The stylesheet cell is about 208 px.
const CELL_HEIGHT: f32 = 208.0;

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
    #[expect(
        clippy::cast_precision_loss,
        reason = "a column count well below 2^23 stays exact in f32"
    )]
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
    #[expect(
        clippy::cast_precision_loss,
        reason = "a row or column count well below 2^23 stays exact in f32"
    )]
    pub fn cell_rect(&self, row: usize, col: usize) -> Rect {
        let stride = self.stride();
        Rect::from_min_size(
            pos2(col as f32 * stride.x, row as f32 * stride.y),
            self.cell,
        )
    }

    /// Stretch the cell width so `columns` tracks fill `width`.
    ///
    /// The cell height stays fixed. The column count is the largest track
    /// count that fits, so a narrow grid keeps one column. This matches the
    /// stylesheet `minmax(196px, 1fr)` track.
    #[must_use]
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss,
        reason = "the column count is a small positive number"
    )]
    pub fn stretched(self, width: f32) -> Self {
        let columns = ((width + self.spacing) / (TRACK_WIDTH + self.spacing))
            .floor()
            .max(1.0) as usize;
        let gaps = self.spacing * (columns.saturating_sub(1)) as f32;
        let cell_width = ((width - gaps) / columns as f32).max(TRACK_WIDTH);
        Self {
            columns,
            cell: vec2(cell_width, self.cell.y),
            spacing: self.spacing,
        }
    }
}

/// The size of one grid cell.
#[must_use]
pub fn cell_size() -> Vec2 {
    vec2(TRACK_WIDTH, CELL_HEIGHT)
}

/// The grid geometry for an available `width`, in points.
///
/// The grid holds at least one column, however narrow the window is. The cell
/// width stretches so the last column fills the row.
#[must_use]
pub fn metrics_for(width: f32) -> GridMetrics {
    GridMetrics {
        columns: 1,
        cell: cell_size(),
        spacing: tokens::GRID_GAP,
    }
    .stretched(width)
}

/// One cell click, with the modifier keys that were held.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CellClick {
    /// Index into the item slice.
    pub index: usize,
    /// Display position of the cell in the order slice.
    pub position: usize,
    /// The modifier keys held when the click arrived.
    pub modifiers: Modifiers,
}

/// The outcome of one painted frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GridResponse {
    /// Number of cells built this frame.
    pub painted: usize,
    /// The cell clicked this frame, if any.
    pub click: Option<CellClick>,
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
    /// Show the grid at a pinned scroll offset and paint only the visible cells.
    ///
    /// `scroll_y` is the vertical scroll position in points. A pinned offset
    /// ignores the scroll state of the area, so the grid cannot move by input.
    /// Use [`Self::show_interactive`] for a grid the user can scroll.
    pub fn show(
        self,
        ui: &mut Ui,
        scroll_y: f32,
        cell_ui: impl FnMut(&mut Ui, Rect, usize),
    ) -> usize {
        self.show_at(ui, scroll_y, cell_ui)
    }

    /// Paint only the visible cells at a pinned `scroll_y` in points.
    ///
    /// The function calls `cell_ui` once for each visible cell. It returns the
    /// number of cells it built. That count proves the grid virtualizes.
    pub fn show_at(
        self,
        ui: &mut Ui,
        scroll_y: f32,
        cell_ui: impl FnMut(&mut Ui, Rect, usize),
    ) -> usize {
        self.paint(ui, Some(scroll_y), cell_ui)
    }

    /// Paint only the visible cells and let the scroll area keep the offset.
    ///
    /// The scroll position lives in egui state across frames, so a wheel turn or
    /// a scrollbar drag moves the grid. Use [`Self::show_at`] to pin the offset.
    pub fn show_interactive(self, ui: &mut Ui, cell_ui: impl FnMut(&mut Ui, Rect, usize)) -> usize {
        self.paint(ui, None, cell_ui)
    }

    /// Paint the visible cells. A `Some` offset pins the scroll position.
    fn paint(
        self,
        ui: &mut Ui,
        scroll_y: Option<f32>,
        mut cell_ui: impl FnMut(&mut Ui, Rect, usize),
    ) -> usize {
        let mut painted = 0usize;

        let area = ScrollArea::vertical().auto_shrink([false, false]);
        let area = match scroll_y {
            Some(scroll_y) => area.scroll_offset(vec2(0.0, scroll_y)),
            None => area,
        };
        area.show_viewport(ui, |ui, viewport| {
            // The inner width excludes the scrollbar, so the columns fit.
            let metrics = self.metrics.stretched(ui.available_width());
            let content = metrics.content_size(self.item_count);
            let row_count = metrics.row_count(self.item_count);
            ui.set_min_size(content);
            let origin = ui.max_rect().min;
            let cols = metrics.visible_cols(viewport);
            for row in metrics.visible_rows(viewport) {
                if row >= row_count {
                    break;
                }
                for col in cols.clone() {
                    let index = row * metrics.columns + col;
                    if index >= self.item_count {
                        break;
                    }
                    let rect = metrics.cell_rect(row, col).translate(origin.to_vec2());
                    cell_ui(ui, rect, index);
                    painted += 1;
                }
            }
        });

        painted
    }

    /// Show the grid over `items` in display `order` and paint each tile.
    ///
    /// `order` holds item indices in display order, as [`items_in_order`]
    /// returns them. The scroll area keeps its own offset, so the user can
    /// scroll the grid. The function reports the clicked item, if a click landed
    /// on a tile.
    pub fn show_items(
        self,
        ui: &mut Ui,
        items: &[GridItem],
        order: &[usize],
        selection: &Selection,
    ) -> GridResponse {
        self.items_at(ui, None, items, order, selection, None)
    }

    /// Like [`Self::show_items`], and push the item index of each painted
    /// tile into `painted`. The caller clears the vector first.
    pub fn show_items_collect(
        self,
        ui: &mut Ui,
        items: &[GridItem],
        order: &[usize],
        selection: &Selection,
        painted: &mut Vec<usize>,
    ) -> GridResponse {
        self.items_at(ui, None, items, order, selection, Some(painted))
    }

    /// Show the grid over `items` at a pinned `scroll_y` in points.
    ///
    /// Use this when the caller owns the scroll position, as a benchmark does.
    pub fn show_items_at(
        self,
        ui: &mut Ui,
        scroll_y: f32,
        items: &[GridItem],
        order: &[usize],
        selection: &Selection,
    ) -> GridResponse {
        self.items_at(ui, Some(scroll_y), items, order, selection, None)
    }

    /// Paint the tiles. A `Some` offset pins the scroll position.
    fn items_at(
        self,
        ui: &mut Ui,
        scroll_y: Option<f32>,
        items: &[GridItem],
        order: &[usize],
        selection: &Selection,
        mut painted_items: Option<&mut Vec<usize>>,
    ) -> GridResponse {
        let mut click = None;
        let grid = Self {
            metrics: self.metrics,
            item_count: order.len(),
        };
        let painted = grid.paint(ui, scroll_y, |ui, rect, position| {
            let Some(&index) = order.get(position) else {
                return;
            };
            let Some(item) = items.get(index) else {
                return;
            };
            if let Some(painted) = painted_items.as_deref_mut() {
                painted.push(index);
            }
            let response = cell::show_cell(ui, rect, item, index, selection);
            if response.clicked() {
                click = Some(CellClick {
                    index,
                    position,
                    modifiers: ui.input(|input| input.modifiers),
                });
            }
        });
        GridResponse { painted, click }
    }
}

/// Index of the first cell row or column that touches a coordinate.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "a non-negative coordinate gives a non-negative index"
)]
fn index_floor(coord: f32, stride: f32) -> usize {
    (coord / stride).floor() as usize
}

/// One past the last cell row or column that touches a coordinate.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "a non-negative coordinate gives a non-negative index"
)]
fn index_ceil(coord: f32, stride: f32) -> usize {
    (coord / stride).ceil() as usize
}
