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
//! This file holds the compositor state machine and the composite region type.
//! The layer and group paint, including the effects path, lives in
//! [`crate::composite_paint`].

use std::mem;

use crate::adjustments::Adjustment;
use crate::blend::{BlendMode, safe_div, union};
use crate::composite_paint::active_effects;
use crate::content::{GradientFill, LayerContent};
use crate::decode::Viewport;
use crate::document::PsdDocument;
use crate::error::PsdError;
use crate::layer_source::{channel_samples, cmyk_color, read_layer, rgb_color};
use crate::layer_tree::{LayerNode, build_tree};
use crate::limits::ParseOptions;
use crate::model::ColorMode;

/// The composited pixels of one viewport.
///
/// Colors are straight (not premultiplied) RGB in `0.0..=1.0`. The alpha is the
/// composite coverage. A fully transparent pixel keeps the white backdrop, so
/// its color is white.
#[derive(Debug, Clone, PartialEq)]
pub struct CompositeRegion {
    /// The viewport this region covers.
    viewport: Viewport,
    /// Straight RGB, three `f32` per pixel, row-major.
    color: Vec<f32>,
    /// Alpha, one `f32` per pixel, row-major.
    alpha: Vec<f32>,
}

impl CompositeRegion {
    /// The viewport this region covers.
    #[must_use]
    pub const fn viewport(&self) -> Viewport {
        self.viewport
    }

    /// The region width in pixels.
    #[must_use]
    pub const fn width(&self) -> u32 {
        self.viewport.width()
    }

    /// The region height in pixels.
    #[must_use]
    pub const fn height(&self) -> u32 {
        self.viewport.height()
    }

    /// The straight RGB at a region pixel.
    ///
    /// Returns `None` when the pixel is outside the region.
    #[must_use]
    pub fn color(&self, x: u32, y: u32) -> Option<[f32; 3]> {
        let index = self.pixel_index(x, y)?;
        Some([
            self.color[index * 3],
            self.color[index * 3 + 1],
            self.color[index * 3 + 2],
        ])
    }

    /// The alpha at a region pixel, or `None` when outside the region.
    #[must_use]
    pub fn alpha(&self, x: u32, y: u32) -> Option<f32> {
        let index = self.pixel_index(x, y)?;
        self.alpha.get(index).copied()
    }

    /// The straight RGBA at a region pixel, or `None` when outside the region.
    #[must_use]
    pub fn pixel(&self, x: u32, y: u32) -> Option<[f32; 4]> {
        let index = self.pixel_index(x, y)?;
        Some([
            self.color[index * 3],
            self.color[index * 3 + 1],
            self.color[index * 3 + 2],
            self.alpha[index],
        ])
    }

    /// Encode the region as straight RGBA8, truncating to 8 bits.
    #[must_use]
    pub fn rgba8(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.alpha.len() * 4);
        for (index, alpha) in self.alpha.iter().enumerate() {
            out.push(to_u8(self.color[index * 3]));
            out.push(to_u8(self.color[index * 3 + 1]));
            out.push(to_u8(self.color[index * 3 + 2]));
            out.push(to_u8(*alpha));
        }
        out
    }

    /// Encode the region as straight RGBA16.
    #[must_use]
    pub fn rgba16(&self) -> Vec<u16> {
        let mut out = Vec::with_capacity(self.alpha.len() * 4);
        for (index, alpha) in self.alpha.iter().enumerate() {
            out.push(to_u16(self.color[index * 3]));
            out.push(to_u16(self.color[index * 3 + 1]));
            out.push(to_u16(self.color[index * 3 + 2]));
            out.push(to_u16(*alpha));
        }
        out
    }

    /// The row-major pixel index of a region pixel.
    #[must_use]
    fn pixel_index(&self, x: u32, y: u32) -> Option<usize> {
        if x >= self.viewport.width() || y >= self.viewport.height() {
            return None;
        }
        let x = usize::try_from(x).ok()?;
        let y = usize::try_from(y).ok()?;
        let stride = usize::try_from(self.viewport.width()).ok()?;
        y.checked_mul(stride)?.checked_add(x)
    }
}

impl PsdDocument {
    /// Composite the document layers over a viewport.
    ///
    /// The region is viewport-sized. The full canvas is never allocated, so
    /// the caller composites one tile at a time. The 120 s deadline and
    /// the 8192 px cap are enforced by the underlying reads.
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
        let color_0 = vec![1.0f32; count];
        let alpha_0 = vec![0.0f32; width * height];
        let mut root = Compositor::new(width, height, color_0, alpha_0, false);
        for node in &build_tree(&self.file().layers) {
            root.apply(node, self, viewport, options)?;
        }
        Ok(CompositeRegion {
            viewport,
            color: root.corrected_color(),
            alpha: root.alpha_g,
        })
    }

    /// Composite the merged preview when the file has no layer records.
    ///
    /// A flattened document falls back to the merged image. The merged data is
    /// a preview over a white background, so the RGB is
    /// un-composited with the alpha channel.
    fn composite_merged_background(
        &self,
        viewport: Viewport,
        options: &ParseOptions,
    ) -> Result<CompositeRegion, PsdError> {
        let region = self.read_merged_region(viewport, options)?;
        let width = usize::try_from(viewport.width()).unwrap_or(0);
        let height = usize::try_from(viewport.height()).unwrap_or(0);
        let count = width
            .checked_mul(height)
            .and_then(|area| area.checked_mul(3))
            .ok_or(PsdError::LengthTooLarge {
                length: u64::from(viewport.width()) * u64::from(viewport.height()),
            })?;
        let area = count / 3;
        let max = if region.depth >= 16 { 65535.0 } else { 255.0 };
        let color = match region.color_mode {
            ColorMode::Rgb => rgb_color(&region, area, max),
            ColorMode::Cmyk => cmyk_color(&region, area, max),
            mode => {
                return Err(PsdError::UnsupportedColorMode {
                    mode,
                    depth: region.depth,
                });
            }
        };
        let alpha_channel = channel_samples(&region, -1);
        let has_alpha = alpha_channel.is_some();
        let alpha = match alpha_channel {
            Some(samples) => samples
                .iter()
                .take(area)
                .map(|sample| f32::from(*sample) / max)
                .collect(),
            None => vec![1.0f32; area],
        };
        let mut color = color;
        // The compositor recovers the straight color only when a merged alpha
        // channel exists and the preview sits on white.
        if region.color_mode == ColorMode::Rgb && has_alpha {
            for index in 0..area {
                let a = alpha[index];
                if a > 0.0 {
                    for channel in 0..3 {
                        let straight = (color[index * 3 + channel] + a - 1.0) / a;
                        color[index * 3 + channel] = straight.clamp(0.0, 1.0);
                    }
                }
            }
        }
        Ok(CompositeRegion {
            viewport,
            color,
            alpha,
        })
    }
}

/// Truncate a normalized value to `u8`.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
#[must_use]
fn to_u8(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0) as u8
}

/// Truncate a normalized value to `u16`.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
#[must_use]
fn to_u16(value: f32) -> u16 {
    (value.clamp(0.0, 1.0) * 65535.0) as u16
}

/// The result of compositing a group's children.
#[derive(Clone)]
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
}

impl Compositor {
    /// Create a compositor over a viewport.
    pub(crate) fn new(
        width: usize,
        height: usize,
        color_0: Vec<f32>,
        alpha_0: Vec<f32>,
        isolated: bool,
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
        }
    }

    /// Apply one layer node.
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
        if node.is_group {
            self.apply_group(node, document, viewport, options)
        } else {
            self.apply_pixels(node, document, viewport, options)
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
        let paint = Self::layer_paint(node, document, viewport, options)?;
        self.apply_source(&paint.color, &paint.shape, &paint.alpha, node.blend);
        Ok(())
    }

    /// Apply one adjustment layer to the running backdrop.
    ///
    /// The adjustment transforms the backdrop, then blends the result back by
    /// the layer opacity times its mask. Alpha is unchanged.
    fn apply_adjustment(
        &mut self,
        node: &LayerNode,
        adjustment: &Adjustment,
        document: &PsdDocument,
        viewport: Viewport,
        options: &ParseOptions,
    ) -> Result<(), PsdError> {
        let Some(index) = node.index else {
            return Ok(());
        };
        let mask = read_layer(document, index, viewport, options)?
            .mask
            .unwrap_or_else(|| vec![1.0f32; self.count]);
        let coverage = node.opacity * node.fill_opacity;
        for (index, &mask_value) in mask.iter().enumerate() {
            let k = coverage * mask_value;
            if k <= 0.0 {
                continue;
            }
            let backdrop = [
                self.color[index * 3],
                self.color[index * 3 + 1],
                self.color[index * 3 + 2],
            ];
            let mut adjusted = backdrop;
            adjustment.apply(&mut adjusted);
            let blended = crate::blend::blend_rgb(node.blend, backdrop, adjusted);
            for channel in 0..3 {
                self.color[index * 3 + channel] =
                    backdrop[channel] + (blended[channel] - backdrop[channel]) * k;
            }
        }
        Ok(())
    }

    /// Render a gradient fill over the viewport.
    ///
    /// The frame is the whole canvas, so tiles agree on the ramp. The returned
    /// color is straight RGB and the shape is the fill alpha.
    #[allow(
        clippy::cast_precision_loss,
        reason = "canvas and viewport coordinates are small enough for f32"
    )]
    pub(crate) fn render_gradient(
        fill: &GradientFill,
        viewport: Viewport,
        document: &PsdDocument,
    ) -> (Vec<f32>, Vec<f32>) {
        let width = usize::try_from(viewport.width()).unwrap_or(0);
        let height = usize::try_from(viewport.height()).unwrap_or(0);
        let mut color = vec![0.0f32; width * height * 3];
        let mut shape = vec![0.0f32; width * height];
        let header = document.file().header;
        let frame = (0.0, 0.0, header.width as f32, header.height as f32);
        for y in 0..height {
            for x in 0..width {
                let canvas_x = viewport.left as f32 + x as f32 + 0.5;
                let canvas_y = viewport.top as f32 + y as f32 + 0.5;
                let sample = fill.sample(fill.parameter_at(frame, canvas_x, canvas_y));
                let index = y * width + x;
                color[index * 3] = sample[0];
                color[index * 3 + 1] = sample[1];
                color[index * 3 + 2] = sample[2];
                shape[index] = sample[3];
            }
        }
        (color, shape)
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
        for index in 0..self.count {
            shape[index] *= node.fill_opacity;
            alpha[index] *= node.fill_opacity;
            opacity_mask[index] *= node.fill_opacity;
        }
        if node.blend == BlendMode::PassThrough && !isolated {
            self.apply_passthrough_source(&color, &shape, &alpha, &opacity_mask);
        } else {
            self.apply_source(&color, &shape, &alpha, node.blend);
        }
        Ok(())
    }

    /// Composite the children of a group and return its result.
    ///
    /// A pass-through group starts from the current backdrop; an isolated group
    /// starts from a transparent buffer. The returned color, coverage, and
    /// alpha are the group's own.
    fn get_group(
        &self,
        node: &LayerNode,
        document: &PsdDocument,
        viewport: Viewport,
        options: &ParseOptions,
    ) -> Result<GroupResult, PsdError> {
        let passthrough = node.blend == BlendMode::PassThrough;
        let isolated_adjustments = node.fill_opacity < 1.0 || !node.clips.is_empty();
        let mut group = Self::new(
            self.width,
            self.height,
            self.color.clone(),
            self.alpha.clone(),
            !passthrough,
        );
        for child in &node.children {
            group.apply(child, document, viewport, options)?;
        }
        let color = if isolated_adjustments {
            group.corrected_color()
        } else {
            group.color.clone()
        };
        Ok(GroupResult {
            color,
            shape: group.shape_g.clone(),
            alpha: group.alpha_g.clone(),
            isolated: isolated_adjustments,
        })
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
    ) -> Result<Vec<f32>, PsdError> {
        let width = usize::try_from(viewport.width()).unwrap_or(0);
        let height = usize::try_from(viewport.height()).unwrap_or(0);
        let mut clip = Self::new(
            width,
            height,
            base_color.to_vec(),
            base_shape.to_vec(),
            false,
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
    pub(crate) fn apply_source(
        &mut self,
        color: &[f32],
        shape: &[f32],
        alpha: &[f32],
        mode: BlendMode,
    ) {
        let backdrop_color = mem::take(&mut self.color);
        let backdrop_alpha = mem::take(&mut self.alpha);
        let mut new_alpha = vec![0.0f32; self.count];
        for index in 0..self.count {
            self.shape_g[index] = union(self.shape_g[index], shape[index]);
            self.alpha_g[index] = union(self.alpha_g[index], alpha[index]);
            new_alpha[index] = union(self.alpha_0[index], self.alpha_g[index]);
        }
        let mut new_color = vec![0.0f32; self.count * 3];
        for index in 0..self.count {
            let out_alpha = new_alpha[index];
            if out_alpha <= 0.0 {
                // The ratio is not finite, so use white.
                new_color[index * 3] = 1.0;
                new_color[index * 3 + 1] = 1.0;
                new_color[index * 3 + 2] = 1.0;
                continue;
            }
            let source_alpha = alpha[index];
            let coverage = shape[index];
            let previous = backdrop_alpha[index];
            let cb = [
                backdrop_color[index * 3],
                backdrop_color[index * 3 + 1],
                backdrop_color[index * 3 + 2],
            ];
            let cs = [color[index * 3], color[index * 3 + 1], color[index * 3 + 2]];
            let blended = crate::blend::blend_rgb(mode, cb, cs);
            for channel in 0..3 {
                let blend_term = (coverage - source_alpha) * previous * cb[channel]
                    + source_alpha * ((1.0 - previous) * cs[channel] + previous * blended[channel]);
                let numerator = (1.0 - coverage) * previous * cb[channel] + blend_term;
                new_color[index * 3 + channel] = (numerator / out_alpha).clamp(0.0, 1.0);
            }
        }
        self.alpha = new_alpha;
        self.color = new_color;
    }

    /// Apply a pass-through group source over the backdrop.
    ///
    /// The group color is blended with the existing backdrop instead of an
    /// isolated buffer.
    fn apply_passthrough_source(
        &mut self,
        color: &[f32],
        shape: &[f32],
        alpha: &[f32],
        mask: &[f32],
    ) {
        let previous_shape = mem::take(&mut self.shape_g);
        let previous_color = mem::take(&mut self.color);
        let previous_alpha_g = mem::take(&mut self.alpha_g);
        let mut new_shape = vec![0.0f32; self.count];
        let mut new_alpha_g = vec![0.0f32; self.count];
        let mut new_alpha = vec![0.0f32; self.count];
        let mut new_color = vec![0.0f32; self.count * 3];
        for index in 0..self.count {
            new_shape[index] = union(previous_shape[index], shape[index]);
            new_alpha_g[index] = union(previous_alpha_g[index], alpha[index]);
            new_alpha[index] = union(self.alpha_0[index], new_alpha_g[index]);
            let mut support = [0.0f32; 3];
            for channel in 0..3 {
                let numerator =
                    color[index * 3 + channel] * mask[index] * (1.0 - previous_shape[index])
                        + previous_color[index * 3 + channel] * previous_shape[index];
                support[channel] = safe_div(numerator, new_shape[index]).clamp(0.0, 1.0);
                let value = color[index * 3 + channel] * mask[index]
                    + (1.0 - mask[index]) * support[channel];
                new_color[index * 3 + channel] = value.clamp(0.0, 1.0);
            }
        }
        self.shape_g = new_shape;
        self.alpha_g = new_alpha_g;
        self.alpha = new_alpha;
        self.color = new_color;
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

#[cfg(test)]
mod tests {
    use super::to_u8;

    #[test]
    fn a_fully_transparent_value_truncates_to_zero() {
        assert_eq!(to_u8(0.0), 0);
        assert_eq!(to_u8(1.0), 255);
        assert_eq!(to_u8(0.5), 127);
    }
}
