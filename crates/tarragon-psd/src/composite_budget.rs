//! The memory budget of one composite call, and the tile size that fits it.
//!
//! A render holds [`COMPOSITE_BASE_BYTES_PER_PIXEL`] bytes per working pixel,
//! plus [`GROUP_BYTES_PER_PIXEL_LEVEL`] for each group nesting level. A layer,
//! clip layer or group with active effects is built over a viewport padded by
//! up to [`crate::effects::MAX_EFFECT_SIZE`] on each side, and holds
//! [`EFFECTS_BYTES_PER_PIXEL`] plus [`EFFECT_PAINT_BYTES_PER_PIXEL`] for each
//! paint it holds on top. A node inside a padded node pads again, so the
//! padding grows with the deepest chain of nodes that have effects. The
//! estimate counts all of that over the padded size, so it is an upper bound
//! for every paint path.

use crate::decode::Viewport;
use crate::document::PsdDocument;
use crate::error::PsdError;
use crate::layer_tree::{LayerNode, build_tree, effects_load, group_depth};
use crate::limits::{
    COMPOSITE_BASE_BYTES_PER_PIXEL, EFFECT_PAINT_BYTES_PER_PIXEL, EFFECTS_BYTES_PER_PIXEL,
    FULL_TIER_MAX_EDGE, GROUP_BYTES_PER_PIXEL_LEVEL, MAX_COMPOSITE_BYTES,
};

/// The padding on each side of a tile, in pixels, for each level of nested
/// effects. It equals [`crate::effects::MAX_EFFECT_SIZE`]; a unit test checks that.
const MAX_PADDING: u64 = 512;

/// The memory model of one document.
#[derive(Debug, Clone, Copy)]
pub(crate) struct TileBudget {
    /// The deepest group nesting.
    depth: usize,
    /// The most nodes with active effects on one root-to-leaf path. Each one
    /// pads the viewport again, so the padding is this many times the cap.
    levels: usize,
    /// The most paints one node holds at once.
    paints: usize,
    /// The canvas size, which bounds the padding.
    canvas: (u32, u32),
}

impl TileBudget {
    /// Read the model from a built tree.
    pub(crate) fn of(tree: &[LayerNode], canvas: (u32, u32)) -> Self {
        let load = effects_load(tree);
        Self {
            depth: group_depth(tree),
            levels: load.padded_levels,
            paints: load.paints,
            canvas,
        }
    }

    /// True when tiles are padded.
    const fn padded(&self) -> bool {
        self.levels > 0
    }

    /// The bytes held for each working pixel.
    fn bytes_per_pixel(&self) -> u64 {
        let effects = if self.padded() {
            EFFECT_PAINT_BYTES_PER_PIXEL
                .saturating_mul(self.paints as u64)
                .saturating_add(EFFECTS_BYTES_PER_PIXEL)
        } else {
            0
        };
        GROUP_BYTES_PER_PIXEL_LEVEL
            .saturating_mul(self.depth as u64 + 1)
            .saturating_add(COMPOSITE_BASE_BYTES_PER_PIXEL)
            .saturating_add(effects)
    }

    /// The padding on each side of a tile, in pixels.
    fn padding(&self) -> u64 {
        MAX_PADDING.saturating_mul(self.levels as u64)
    }

    /// The working size of a tile: the tile plus its padding, bounded by the
    /// canvas.
    fn working_size(&self, width: u32, height: u32) -> (u64, u64) {
        let grow = |edge: u32, canvas: u32| {
            let edge = u64::from(edge);
            if self.padded() {
                edge.saturating_add(2 * self.padding())
                    .min(u64::from(canvas).max(edge))
            } else {
                edge
            }
        };
        (grow(width, self.canvas.0), grow(height, self.canvas.1))
    }

    /// The estimated bytes of a render over a `width` by `height` tile.
    pub(crate) fn estimate(&self, width: u32, height: u32) -> u64 {
        let (w, h) = self.working_size(width, height);
        w.saturating_mul(h).saturating_mul(self.bytes_per_pixel())
    }

    /// The most working pixels the budget allows.
    pub(crate) fn max_pixels(&self) -> u64 {
        MAX_COMPOSITE_BYTES / self.bytes_per_pixel()
    }

    /// Refuse a viewport whose estimate is over the budget.
    pub(crate) fn check(&self, viewport: Viewport) -> Result<(), PsdError> {
        let estimated = self.estimate(viewport.width(), viewport.height());
        if estimated > MAX_COMPOSITE_BYTES {
            return Err(PsdError::OverBudget {
                estimated,
                budget: MAX_COMPOSITE_BYTES,
                max_tile_pixels: self.max_pixels(),
            });
        }
        Ok(())
    }

    /// The largest square tile edge that fits the budget, or 1 when even a
    /// one pixel tile does not.
    ///
    /// The edge keeps room for the padding inside [`FULL_TIER_MAX_EDGE`].
    pub(crate) fn suggested_edge(&self) -> u32 {
        let padding = 2 * self.padding();
        let cap = u64::from(FULL_TIER_MAX_EDGE).saturating_sub(padding).max(1);
        let canvas = u64::from(self.canvas.0.max(self.canvas.1)).max(1);
        let (mut low, mut high) = (1u64, cap.min(canvas));
        while low < high {
            let mid = (low + high).div_ceil(2);
            let edge = u32::try_from(mid).unwrap_or(u32::MAX);
            if self.estimate(edge, edge) <= MAX_COMPOSITE_BYTES {
                low = mid;
            } else {
                high = mid - 1;
            }
        }
        u32::try_from(low).unwrap_or(1)
    }
}

impl PsdDocument {
    /// Refuse a viewport whose estimate is over the budget, for the layer
    /// `tree` (empty for a flattened document).
    pub(crate) fn check_tile_budget(
        &self,
        tree: &[LayerNode],
        viewport: Viewport,
    ) -> Result<(), PsdError> {
        let header = self.file().header;
        TileBudget::of(tree, (header.width, header.height)).check(viewport)
    }

    /// Build the memory model from the document layers.
    fn tile_budget(&self) -> Result<TileBudget, PsdError> {
        let tree = build_tree(&self.file().layers)?;
        let header = self.file().header;
        Ok(TileBudget::of(&tree, (header.width, header.height)))
    }

    /// The most working pixels one `composite_region` call may hold.
    ///
    /// The count is the viewport plus its padding. A document with a layer,
    /// clip layer or group that has effects pads each side of the viewport by up to
    /// `MAX_EFFECT_SIZE`, so the viewport itself must be smaller. Use
    /// [`PsdDocument::suggested_tile_edge`] for a ready tile size.
    ///
    /// # Errors
    ///
    /// Returns [`PsdError::GroupsTooDeep`] when the groups nest deeper than the
    /// compositor accepts.
    pub fn max_tile_pixels(&self) -> Result<u64, PsdError> {
        Ok(self.tile_budget()?.max_pixels())
    }

    /// The largest square tile edge that `composite_region` accepts.
    ///
    /// The edge counts the padding of a group with effects and stays inside
    /// the 8192 px cap with that padding. The result is 1 when even a one
    /// pixel tile is over the budget, and `composite_region` then returns
    /// [`PsdError::OverBudget`].
    ///
    /// # Errors
    ///
    /// Returns [`PsdError::GroupsTooDeep`] when the groups nest deeper than the
    /// compositor accepts.
    pub fn suggested_tile_edge(&self) -> Result<u32, PsdError> {
        Ok(self.tile_budget()?.suggested_edge())
    }
}

#[cfg(test)]
mod tests {
    use super::TileBudget;
    use crate::limits::{FULL_TIER_MAX_EDGE, MAX_COMPOSITE_BYTES};

    fn plain(depth: usize, canvas: (u32, u32)) -> TileBudget {
        TileBudget {
            depth,
            levels: 0,
            paints: 0,
            canvas,
        }
    }

    #[test]
    fn the_padding_matches_the_effect_size_cap() {
        assert!(
            (crate::effects::MAX_EFFECT_SIZE
                - f32::from(u16::try_from(super::MAX_PADDING).expect("fits")))
            .abs()
                < f32::EPSILON
        );
    }

    #[test]
    fn the_estimate_is_forty_per_level_plus_the_base() {
        assert_eq!(plain(0, (100, 100)).estimate(10, 10), 100 * 96);
        assert_eq!(plain(3, (100, 100)).estimate(10, 10), 100 * 216);
    }

    #[test]
    fn the_suggested_edge_fits_and_one_more_does_not() {
        for depth in [0, 3, 32] {
            let budget = plain(depth, (8192, 8192));
            let edge = budget.suggested_edge();
            assert!(budget.estimate(edge, edge) <= MAX_COMPOSITE_BYTES);
            if edge < 8192 {
                assert!(budget.estimate(edge + 1, edge + 1) > MAX_COMPOSITE_BYTES);
            }
        }
    }

    #[test]
    fn padding_shrinks_the_tile_and_counts_in_the_estimate() {
        let padded = TileBudget {
            depth: 3,
            levels: 1,
            paints: 1,
            canvas: (8192, 8192),
        };
        assert_eq!(padded.estimate(100, 100), 1124 * 1124 * (216 + 160 + 16));
        let edge = padded.suggested_edge();
        assert!(edge < plain(3, (8192, 8192)).suggested_edge());
        assert!(padded.estimate(edge, edge) <= MAX_COMPOSITE_BYTES);
        assert!(padded.estimate(edge + 1, edge + 1) > MAX_COMPOSITE_BYTES);
    }

    #[test]
    fn each_held_paint_adds_its_bytes() {
        let one = TileBudget {
            depth: 0,
            levels: 1,
            paints: 1,
            canvas: (8192, 8192),
        };
        let three = TileBudget { paints: 3, ..one };
        assert_eq!(one.estimate(100, 100), 1124 * 1124 * (96 + 160 + 16));
        assert_eq!(three.estimate(100, 100), 1124 * 1124 * (96 + 160 + 48));
    }

    #[test]
    fn nested_effects_pad_once_per_level() {
        let nested = |canvas| TileBudget {
            depth: 1,
            levels: 2,
            paints: 1,
            canvas: (canvas, canvas),
        };
        let large = nested(8192);
        assert_eq!(large.estimate(100, 100), 2148 * 2148 * (80 + 56 + 160 + 16));
        // Two levels of padding leave no room at this canvas size.
        assert_eq!(large.suggested_edge(), 1);
        // A smaller canvas bounds the padding, so the whole canvas fits.
        let small = nested(1800);
        assert_eq!(small.estimate(100, 100), 1800 * 1800 * 312);
        let edge = small.suggested_edge();
        assert_eq!(edge, 1800);
        assert!(small.estimate(edge, edge) <= MAX_COMPOSITE_BYTES);
    }

    #[test]
    fn one_more_level_of_padding_shrinks_the_suggested_edge() {
        let at = |levels| TileBudget {
            depth: 1,
            levels,
            paints: 1,
            canvas: (8192, 8192),
        };
        let (none, one) = (at(0).suggested_edge(), at(1).suggested_edge());
        assert!(one < none);
        assert!(one + 2 * 512 <= FULL_TIER_MAX_EDGE);
        assert!(at(1).estimate(one, one) <= MAX_COMPOSITE_BYTES);
        assert!(at(1).estimate(one + 1, one + 1) > MAX_COMPOSITE_BYTES);
    }

    #[test]
    fn the_suggested_edge_fits_for_every_nesting_level() {
        for levels in 0..=4 {
            let budget = TileBudget {
                depth: levels,
                levels,
                paints: 6,
                canvas: (1000, 1000),
            };
            let edge = budget.suggested_edge();
            assert!(budget.estimate(edge, edge) <= MAX_COMPOSITE_BYTES);
        }
    }

    #[test]
    fn padding_never_grows_past_the_canvas() {
        let padded = TileBudget {
            depth: 0,
            levels: 1,
            paints: 1,
            canvas: (200, 200),
        };
        assert_eq!(padded.estimate(100, 100), 200 * 200 * (96 + 160 + 16));
    }

    #[test]
    fn a_hopeless_budget_suggests_one_pixel() {
        let deep = TileBudget {
            depth: 32,
            levels: 1,
            paints: 1,
            canvas: (8192, 8192),
        };
        assert_eq!(deep.suggested_edge(), 1);
    }
}
