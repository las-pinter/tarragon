//! Layer compositor for PSD and PSB documents.
//!
//! The compositor reads layers in order, bottom to top, and composites each one
//! onto a viewport-sized canvas. It honors visibility, opacity, fill opacity,
//! blend mode, layer masks, clipping masks, groups, pass-through groups, and
//! layer effects. It converts CMYK to RGB and normalizes 8-bit and 16-bit
//! samples to `f32`.
//!
//! The work is tiled. The caller passes one [`Viewport`]; every buffer is
//! viewport-sized, and the full canvas is never allocated. The blend formulas
//! live in [`crate::blend`].
//!
//! A layer effect paints behind the layer or over it, depending on the effect
//! kind. A tile with an effect pads the read by the effect reach before the
//! coverage is built, so no tile boundary cuts a shadow, a glow, or a stroke.
//! Vector masks parse but are not rasterized. Adjustment and fill descriptors
//! parse and apply.
//!
//! This file holds the compositor state machine. The layer and group paint,
//! including the effects path, lives in [`crate::composite_paint`]. The fills,
//! the adjustments, and the merged-preview fallback live in
//! `composite_fill`.

use std::cell::Cell;
use std::mem;
use std::rc::Rc;

use crate::blend::{BlendMode, fill_is_special, safe_div, union};
use crate::composite_paint::active_effects;
pub use crate::composite_region::CompositeRegion;
use crate::content::LayerContent;
use crate::decode::Viewport;
use crate::document::PsdDocument;
use crate::error::PsdError;
use crate::layer_source::read_layer;
use crate::layer_tree::{LayerNode, build_tree};
use crate::limits::ParseOptions;
use crate::model::sample_bytes;

impl PsdDocument {
    /// Composite the document layers over a viewport.
    ///
    /// The region is viewport-sized. The full canvas is never allocated, so
    /// the caller composites one tile at a time. The 120 s deadline and
    /// the 8192 px cap are enforced by the underlying reads.
    ///
    /// # Tiling contract
    ///
    /// The memory estimate is checked against [`crate::limits::MAX_COMPOSITE_BYTES`]
    /// before any plane is allocated, for flattened documents too. The caller
    /// tiles the canvas so each tile fits: [`PsdDocument::max_tile_pixels`]
    /// gives the pixel count and [`PsdDocument::suggested_tile_edge`] a square
    /// edge. A layer, clip layer or group with effects pads the viewport by up
    /// to `MAX_EFFECT_SIZE` on each side, and the padding counts in the
    /// estimate. The tile edge plus the padding must also stay within
    /// [`crate::limits::FULL_TIER_MAX_EDGE`].
    ///
    /// # Errors
    ///
    /// Returns a typed error when the viewport is invalid or exceeds the cap,
    /// the color mode is unsupported, the deadline passed, or the bytes are
    /// truncated.
    pub fn composite_region(
        &self,
        viewport: Viewport,
        options: &ParseOptions,
    ) -> Result<CompositeRegion, PsdError> {
        self.file().check_region(viewport)?;
        // A flattened document has no layer records, so the compositor falls
        // back to the merged preview image (the background).
        if self.file().layers.is_empty() {
            self.check_tile_budget(&[], viewport)?;
            return self.composite_merged_background(viewport, options);
        }
        let width = usize::try_from(viewport.width()).unwrap_or(0);
        let height = usize::try_from(viewport.height()).unwrap_or(0);
        let count = width
            .checked_mul(height)
            .and_then(|area| area.checked_mul(3))
            .ok_or(PsdError::LengthTooLarge {
                length: u64::from(viewport.width()) * u64::from(viewport.height()),
            })?;
        // The tree is built first, so a file that nests groups too deep fails
        // before any viewport buffer exists.
        let tree = build_tree(&self.file().layers)?;
        self.check_tile_budget(&tree, viewport)?;
        // The color mode is a property of the document, so an unsupported mode
        // fails the render. Only a single unsupported layer is skipped.
        let header = self.file().header;
        sample_bytes(header.color_mode, header.depth)?;
        let color_0 = vec![1.0f32; count];
        let alpha_0 = vec![0.0f32; width * height];
        let skips = Rc::new(Cell::new(0));
        let mut root = Compositor::new(width, height, color_0, alpha_0, false, &skips);
        for node in &tree {
            root.apply(node, self, viewport, options)?;
        }
        Ok(CompositeRegion::new(
            viewport,
            root.corrected_color(),
            root.alpha_g,
            skips.get(),
        ))
    }
}

/// The count of layers skipped for an unsupported compression.
pub(crate) type SkipCount = Rc<Cell<usize>>;

/// The result of compositing a group's children.
struct GroupResult {
    /// The group color, three `f32` per pixel.
    color: Vec<f32>,
    /// The group coverage, one `f32` per pixel.
    shape: Vec<f32>,
    /// The group alpha, one `f32` per pixel.
    alpha: Vec<f32>,
    /// True when adjustments are isolated from the backdrop.
    isolated: bool,
}

/// The running compositor state over one viewport.
///
/// `color_0` and `alpha_0` are the initial backdrop, `shape_g` and `alpha_g`
/// are the accumulated group coverage and alpha, and `color` and `alpha` are
/// the running accumulators. `clip_base` holds the base layer index when this
/// compositor paints the clipping layers of that base.
#[derive(Clone)]
pub(crate) struct Compositor {
    /// The viewport width.
    pub(crate) width: usize,
    /// The viewport height.
    pub(crate) height: usize,
    /// The pixel count.
    pub(crate) count: usize,
    /// The initial backdrop color, three `f32` per pixel.
    pub(crate) color_0: Vec<f32>,
    /// The initial backdrop alpha, one `f32` per pixel.
    pub(crate) alpha_0: Vec<f32>,
    /// The accumulated coverage, one `f32` per pixel.
    pub(crate) shape_g: Vec<f32>,
    /// The accumulated alpha, one `f32` per pixel.
    pub(crate) alpha_g: Vec<f32>,
    /// The running color, three `f32` per pixel.
    pub(crate) color: Vec<f32>,
    /// The running alpha, one `f32` per pixel.
    pub(crate) alpha: Vec<f32>,
    /// The base layer index of a clipping stack, when this compositor paints
    /// the clipping layers of that base.
    pub(crate) clip_base: Option<usize>,
    /// The shared count of skipped layers.
    pub(crate) skips: SkipCount,
}

impl Compositor {
    /// Create a compositor over a viewport.
    pub(crate) fn new(
        width: usize,
        height: usize,
        color_0: Vec<f32>,
        alpha_0: Vec<f32>,
        isolated: bool,
        skips: &SkipCount,
    ) -> Self {
        let count = width * height;
        let alpha_0 = if isolated {
            vec![0.0f32; count]
        } else {
            alpha_0
        };
        Self {
            width,
            height,
            count,
            color: color_0.clone(),
            alpha: alpha_0.clone(),
            color_0,
            alpha_0,
            shape_g: vec![0.0f32; count],
            alpha_g: vec![0.0f32; count],
            clip_base: None,
            skips: Rc::clone(skips),
        }
    }

    /// Start a nested group by moving this state's backdrop into it.
    ///
    /// The group keeps the moved planes as its initial backdrop, so only the
    /// running planes are new. [`Self::finish_nested`] moves them back. An
    /// isolated group starts from a transparent alpha, so the parent alpha is
    /// returned to be kept aside.
    fn begin_nested(&mut self, isolated: bool) -> (Self, Option<Vec<f32>>) {
        let color_0 = mem::take(&mut self.color);
        let parent_alpha = mem::take(&mut self.alpha);
        let (alpha_0, alpha, kept) = if isolated {
            (
                vec![0.0f32; self.count],
                vec![0.0f32; self.count],
                Some(parent_alpha),
            )
        } else {
            (parent_alpha.clone(), parent_alpha, None)
        };
        let group = Self {
            width: self.width,
            height: self.height,
            count: self.count,
            color: color_0.clone(),
            alpha,
            color_0,
            alpha_0,
            shape_g: vec![0.0f32; self.count],
            alpha_g: vec![0.0f32; self.count],
            clip_base: None,
            skips: Rc::clone(&self.skips),
        };
        (group, kept)
    }

    /// Give the backdrop back to the parent after a nested group.
    ///
    /// A pass-through group moved the parent alpha into `alpha_0`; an isolated
    /// group has it in `kept`.
    fn finish_nested(&mut self, group: &mut Self, kept: Option<Vec<f32>>) {
        self.color = mem::take(&mut group.color_0);
        self.alpha = kept.unwrap_or_else(|| mem::take(&mut group.alpha_0));
    }

    /// Apply one layer node.
    ///
    /// A layer whose channels use an unsupported compression is skipped and
    /// counted, so one such layer does not fail the whole render. A layer reads
    /// its channels before it changes the state, so a skipped layer leaves no
    /// partial paint. The color mode is checked once before the walk, so it
    /// cannot fail here.
    pub(crate) fn apply(
        &mut self,
        node: &LayerNode,
        document: &PsdDocument,
        viewport: Viewport,
        options: &ParseOptions,
    ) -> Result<(), PsdError> {
        if !node.visible {
            return Ok(());
        }
        let result = if node.is_group {
            self.apply_group(node, document, viewport, options)
        } else {
            self.apply_pixels(node, document, viewport, options)
        };
        match result {
            Err(PsdError::UnsupportedCompression(_)) => {
                self.skips.set(self.skips.get() + 1);
                Ok(())
            }
            other => other,
        }
    }

    /// Composite a pixel, fill, or adjustment layer.
    fn apply_pixels(
        &mut self,
        node: &LayerNode,
        document: &PsdDocument,
        viewport: Viewport,
        options: &ParseOptions,
    ) -> Result<(), PsdError> {
        if node.index.is_none() {
            return Ok(());
        }
        if let LayerContent::Adjustment(adjustment) = &node.content {
            return self.apply_adjustment(node, adjustment, document, viewport, options);
        }
        // A recognized block the compositor does not apply yet contributes nothing.
        if node.content == LayerContent::Unsupported {
            return Ok(());
        }
        if let Some(effects) = active_effects(node) {
            return self.apply_pixels_with_effects(node, effects, document, viewport, options);
        }
        let paint = Self::layer_paint(node, document, viewport, options, &self.skips)?;
        if node.fill_opacity < 1.0 && fill_is_special(node.blend) {
            let over: Vec<f32> = paint.geom.iter().map(|g| g * node.opacity).collect();
            self.apply_source_fill(
                &paint.color,
                &paint.shape,
                &paint.alpha,
                &over,
                node.blend,
                node.fill_opacity,
            );
        } else {
            self.apply_source(&paint.color, &paint.shape, &paint.alpha, node.blend);
        }
        Ok(())
    }

    /// Composite a group layer.
    fn apply_group(
        &mut self,
        node: &LayerNode,
        document: &PsdDocument,
        viewport: Viewport,
        options: &ParseOptions,
    ) -> Result<(), PsdError> {
        if let Some(effects) = active_effects(node) {
            return self.apply_group_with_effects(node, effects, document, viewport, options);
        }
        let GroupResult {
            color,
            shape,
            alpha,
            isolated,
        } = self.get_group(node, document, viewport, options)?;
        let mask = match node.index {
            Some(index)
                if document
                    .file()
                    .layers
                    .get(index)
                    .is_some_and(|l| l.mask.is_some()) =>
            {
                read_layer(document, index, viewport, options)?.mask
            }
            _ => None,
        };
        let mask = mask.unwrap_or_else(|| vec![1.0f32; self.count]);
        let mut shape = shape;
        let mut alpha = alpha;
        let mut opacity_mask = mask.clone();
        for index in 0..self.count {
            shape[index] *= mask[index];
            alpha[index] *= mask[index] * node.opacity;
            opacity_mask[index] *= node.opacity;
        }
        // The special-eight modes weigh the backdrop by the alpha before fill.
        let over = alpha.clone();
        for index in 0..self.count {
            shape[index] *= node.fill_opacity;
            alpha[index] *= node.fill_opacity;
            opacity_mask[index] *= node.fill_opacity;
        }
        if node.blend == BlendMode::PassThrough && !isolated {
            self.apply_passthrough_source(&color, &shape, &alpha, &opacity_mask);
        } else if node.fill_opacity < 1.0 && fill_is_special(node.blend) {
            self.apply_source_fill(&color, &shape, &alpha, &over, node.blend, node.fill_opacity);
        } else {
            self.apply_source(&color, &shape, &alpha, node.blend);
        }
        Ok(())
    }

    /// Composite the children of a group and return its result.
    ///
    /// A pass-through group starts from the current backdrop; an isolated group
    /// starts from a transparent buffer. The returned color, coverage, and
    /// alpha are the group's own. The group borrows this state's backdrop
    /// planes by moving them, and gives them back before it returns, so a
    /// nesting level holds only its running planes.
    fn get_group(
        &mut self,
        node: &LayerNode,
        document: &PsdDocument,
        viewport: Viewport,
        options: &ParseOptions,
    ) -> Result<GroupResult, PsdError> {
        let passthrough = node.blend == BlendMode::PassThrough;
        let isolated_adjustments = node.fill_opacity < 1.0 || !node.clips.is_empty();
        let (mut group, kept) = self.begin_nested(!passthrough);
        let mut failure = None;
        for child in &node.children {
            if let Err(error) = group.apply(child, document, viewport, options) {
                failure = Some(error);
                break;
            }
        }
        let color = if isolated_adjustments {
            group.corrected_color()
        } else {
            mem::take(&mut group.color)
        };
        let result = GroupResult {
            color,
            shape: mem::take(&mut group.shape_g),
            alpha: mem::take(&mut group.alpha_g),
            isolated: isolated_adjustments,
        };
        self.finish_nested(&mut group, kept);
        match failure {
            Some(error) => Err(error),
            None => Ok(result),
        }
    }

    /// Composite the clipping layers of a base layer over its color.
    ///
    /// The base coverage and alpha stay unchanged, so clipping restricts the
    /// clip layers to the base shape. The clip compositor remembers the base
    /// layer index, so an effect on a clipped layer also stops at the base
    /// shape.
    pub(crate) fn clip_color(
        node: &LayerNode,
        base_color: &[f32],
        base_shape: &[f32],
        document: &PsdDocument,
        viewport: Viewport,
        options: &ParseOptions,
        skips: &SkipCount,
    ) -> Result<Vec<f32>, PsdError> {
        let width = usize::try_from(viewport.width()).unwrap_or(0);
        let height = usize::try_from(viewport.height()).unwrap_or(0);
        let mut clip = Self::new(
            width,
            height,
            base_color.to_vec(),
            base_shape.to_vec(),
            false,
            skips,
        );
        clip.clip_base = node.index;
        for layer in &node.clips {
            clip.apply(layer, document, viewport, options)?;
        }
        Ok(clip.color)
    }

    /// Apply one source over the backdrop with a blend mode.
    ///
    /// The shape term may differ from the alpha when fill opacity is below one.
    /// The planes update in place, because each pixel reads only its own old
    /// values.
    pub(crate) fn apply_source(
        &mut self,
        color: &[f32],
        shape: &[f32],
        alpha: &[f32],
        mode: BlendMode,
    ) {
        self.apply_source_with(color, shape, alpha, mode, None);
    }

    /// Apply one source whose fill opacity changes the blend itself.
    ///
    /// `over` is the source alpha before fill, and `alpha` is that alpha times
    /// the fill. `shape` is the coverage for the group planes. For the special
    /// eight modes below full fill, the backdrop sees `over` as its weight and a source pulled toward the neutral colour; the
    /// `alpha` plane only grows the result alpha. Any other mode takes the
    /// plain coverage path.
    pub(crate) fn apply_source_fill(
        &mut self,
        color: &[f32],
        shape: &[f32],
        alpha: &[f32],
        over: &[f32],
        mode: BlendMode,
        fill: f32,
    ) {
        if fill >= 1.0 || !fill_is_special(mode) {
            self.apply_source(color, shape, alpha, mode);
        } else {
            self.apply_source_with(color, shape, alpha, mode, Some((over, fill)));
        }
    }

    /// The per-pixel blend behind [`Self::apply_source`] and
    /// [`Self::apply_source_fill`].
    fn apply_source_with(
        &mut self,
        color: &[f32],
        shape: &[f32],
        alpha: &[f32],
        mode: BlendMode,
        special: Option<(&[f32], f32)>,
    ) {
        for index in 0..self.count {
            self.shape_g[index] = union(self.shape_g[index], shape[index]);
            self.alpha_g[index] = union(self.alpha_g[index], alpha[index]);
            let out_alpha = union(self.alpha_0[index], self.alpha_g[index]);
            let previous = self.alpha[index];
            self.alpha[index] = out_alpha;
            if out_alpha <= 0.0 {
                // The ratio is not finite, so use white.
                self.color[index * 3..index * 3 + 3].fill(1.0);
                continue;
            }
            let source_alpha = alpha[index];
            let coverage = shape[index];
            let cb = [
                self.color[index * 3],
                self.color[index * 3 + 1],
                self.color[index * 3 + 2],
            ];
            let cs = [color[index * 3], color[index * 3 + 1], color[index * 3 + 2]];
            if let Some((over, fill)) = special {
                let over = over[index];
                if over <= 0.0 {
                    continue;
                }
                let blended = crate::blend::blend_rgb_fill(mode, cb, cs, fill);
                for channel in 0..3 {
                    let numerator = previous * (1.0 - over) * cb[channel]
                        + previous * over * blended[channel]
                        + (1.0 - previous) * source_alpha * cs[channel];
                    self.color[index * 3 + channel] = (numerator / out_alpha).clamp(0.0, 1.0);
                }
                continue;
            }
            let blended = crate::blend::blend_rgb(mode, cb, cs);
            for channel in 0..3 {
                let blend_term = (coverage - source_alpha) * previous * cb[channel]
                    + source_alpha * ((1.0 - previous) * cs[channel] + previous * blended[channel]);
                let numerator = (1.0 - coverage) * previous * cb[channel] + blend_term;
                self.color[index * 3 + channel] = (numerator / out_alpha).clamp(0.0, 1.0);
            }
        }
    }

    /// Apply a pass-through group source over the backdrop.
    ///
    /// The group color is blended with the existing backdrop instead of an
    /// isolated buffer. The planes update in place, because each pixel reads
    /// only its own old values.
    fn apply_passthrough_source(
        &mut self,
        color: &[f32],
        shape: &[f32],
        alpha: &[f32],
        mask: &[f32],
    ) {
        for index in 0..self.count {
            let previous_shape = self.shape_g[index];
            let new_shape = union(previous_shape, shape[index]);
            let new_alpha_g = union(self.alpha_g[index], alpha[index]);
            self.shape_g[index] = new_shape;
            self.alpha_g[index] = new_alpha_g;
            self.alpha[index] = union(self.alpha_0[index], new_alpha_g);
            for channel in 0..3 {
                let slot = index * 3 + channel;
                let numerator = color[slot] * mask[index] * (1.0 - previous_shape)
                    + self.color[slot] * previous_shape;
                let support = safe_div(numerator, new_shape).clamp(0.0, 1.0);
                let value = color[slot] * mask[index] + (1.0 - mask[index]) * support;
                self.color[slot] = value.clamp(0.0, 1.0);
            }
        }
    }

    /// The final color with the backdrop-alpha correction.
    ///
    /// A transparent result keeps the white backdrop.
    #[must_use]
    pub(crate) fn corrected_color(&self) -> Vec<f32> {
        let mut out = self.color.clone();
        for index in 0..self.count {
            let ratio = safe_div(self.alpha_0[index], self.alpha_g[index]);
            let factor = ratio - self.alpha_0[index];
            for channel in 0..3 {
                let value = out[index * 3 + channel]
                    + (out[index * 3 + channel] - self.color_0[index * 3 + channel]) * factor;
                out[index * 3 + channel] = value.clamp(0.0, 1.0);
            }
        }
        out
    }
}
