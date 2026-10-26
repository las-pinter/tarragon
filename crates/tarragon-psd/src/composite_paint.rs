//! Layer and group paint for the compositor, including the effects path.
//!
//! A layer with effects is painted over a viewport padded by the effect reach.
//! The exterior effects composite behind the layer content, the interior
//! effects paint over it, and the whole stack then composites at the layer
//! opacity. A group is built in isolation over the padded viewport, so an
//! exterior effect sees the whole group edge.
//!
//! # Fill opacity
//!
//! Fill opacity scales only the pixels of the layer itself. The interior
//! effects keep full strength inside the layer shape, scaled by their own
//! opacity and by the layer opacity, so they stay visible at a fill of zero.
//! This is the Photoshop default with "Blend Interior Effects as Group" off.
//! The exterior effects do not change. A drop shadow still sees through the
//! layer by the amount of fill. For the blend modes where fill is not plain
//! coverage, the stack goes through the fill blend of the layer mode.
//!
//! # Tile size and padding
//!
//! The padded viewport must stay inside the full-tier cap on each axis. A tile
//! keeps its padding when its edge plus twice the effect reach is at most
//! [`FULL_TIER_MAX_EDGE`]. A larger tile loses the padding that does not fit,
//! and an exterior effect can then seam at that tile boundary.
//!
//! One file holds the paint of a layer and of a group, so the two paths share
//! the same stack and opacity rules.

use crate::blend::union;
use crate::composite::{Compositor, SkipCount};
use crate::composite_interior::LayerStage;
use crate::content::LayerContent;
use crate::decode::Viewport;
use crate::document::PsdDocument;
use crate::effects::Effects;
use crate::effects_apply::{
    Frame, PaintContext, crop_color, crop_paint, crop_plane, emboss_paints, exterior_paints,
};
use crate::effects_interior::paint_interior;
use crate::effects_map::Coverage;
use crate::error::PsdError;
use crate::layer::LayerBounds;
use crate::layer_source::{read_layer, solid_color};
use crate::layer_tree::LayerNode;
use crate::limits::{FULL_TIER_MAX_EDGE, ParseOptions};

/// The painted planes of one layer over one viewport.
pub(crate) struct LayerPaint {
    /// Straight RGB, three `f32` per pixel.
    pub(crate) color: Vec<f32>,
    /// The geometric coverage before fill opacity, one `f32` per pixel.
    pub(crate) geom: Vec<f32>,
    /// The source coverage after fill opacity, one `f32` per pixel.
    pub(crate) shape: Vec<f32>,
    /// The source alpha after opacity and fill opacity, one `f32` per pixel.
    pub(crate) alpha: Vec<f32>,
}

impl Compositor {
    /// Composite a pixel or fill layer that carries enabled effects.
    pub(crate) fn apply_pixels_with_effects(
        &mut self,
        node: &LayerNode,
        effects: &Effects,
        document: &PsdDocument,
        viewport: Viewport,
        options: &ParseOptions,
    ) -> Result<(), PsdError> {
        let Some(index) = node.index else {
            return Ok(());
        };
        let canvas = (document.file().header.width, document.file().header.height);
        let padded = pad_viewport(viewport, effects.margin(), canvas);
        let paint = Self::layer_paint(node, document, padded, options, &self.skips)?;
        let layer = document
            .file()
            .layers
            .get(index)
            .ok_or(PsdError::LayerOutOfRange {
                index,
                count: document.file().layers.len(),
            })?;
        let layer_bounds = layer_frame(layer.bounds);
        self.apply_paint_with_effects(
            node,
            paint,
            effects,
            layer_bounds,
            canvas,
            padded,
            viewport,
            document,
            options,
        )
    }

    /// Composite a group that carries enabled effects.
    ///
    /// The group is built in isolation over the padded viewport. A pass-through
    /// group with effects loses its pass-through blend against the lower
    /// backdrop; the group result is isolated first.
    pub(crate) fn apply_group_with_effects(
        &mut self,
        node: &LayerNode,
        effects: &Effects,
        document: &PsdDocument,
        viewport: Viewport,
        options: &ParseOptions,
    ) -> Result<(), PsdError> {
        let canvas = (document.file().header.width, document.file().header.height);
        let padded = pad_viewport(viewport, effects.margin(), canvas);
        let paint = Self::group_paint(node, document, padded, options, &self.skips)?;
        let layer_bounds = layer_frame(node.bounds);
        self.apply_paint_with_effects(
            node,
            paint,
            effects,
            layer_bounds,
            canvas,
            padded,
            viewport,
            document,
            options,
        )
    }

    /// Composite one layer paint plus its effects over the backdrop.
    ///
    /// The exterior effects and the layer content compose in a copy of the
    /// running state. The copy then blends back at the layer opacity, so the
    /// opacity applies to the whole stack once.
    #[allow(
        clippy::too_many_arguments,
        reason = "the painted layer needs its stack, its frames, and the node"
    )]
    fn apply_paint_with_effects(
        &mut self,
        node: &LayerNode,
        paint: LayerPaint,
        effects: &Effects,
        layer_bounds: Frame,
        canvas: (u32, u32),
        padded: Viewport,
        viewport: Viewport,
        document: &PsdDocument,
        options: &ParseOptions,
    ) -> Result<(), PsdError> {
        let shape = self.effect_shape(&paint.geom, padded, document, options)?;
        let context = PaintContext {
            viewport: padded,
            layer_bounds,
            canvas,
            global_light: document.file().global_light,
        };
        let mut work = self.clone();
        let mut stack_shape = vec![0.0f32; self.count];
        let mut stack_alpha = vec![0.0f32; self.count];
        // Exterior effects sit behind the layer content.
        for exterior in exterior_paints(effects, &shape, node.fill_opacity, context, options)? {
            apply_exterior(
                &mut work,
                &exterior,
                padded,
                viewport,
                &mut stack_shape,
                &mut stack_alpha,
            );
        }
        // Interior effects paint over the layer content, at full strength.
        let mut color = paint.color;
        let own = (node.fill_opacity < 1.0).then(|| color.clone());
        let mut strength = vec![0.0f32; shape.values.len()];
        paint_interior(&mut color, &mut strength, effects, &shape, context, options)?;
        let color = crop_color(&color, padded, viewport);
        let effect_alpha: Vec<f32> = strength
            .iter()
            .zip(&shape.values)
            .map(|(level, inside)| level * inside)
            .collect();
        let stage = LayerStage {
            color,
            own: own.map(|own| crop_color(&own, padded, viewport)),
            layer_alpha: crop_plane(&paint.shape, padded, viewport),
            over: crop_plane(&paint.geom, padded, viewport),
            effect_alpha: crop_plane(&effect_alpha, padded, viewport),
            blend: node.blend,
            fill: node.fill_opacity,
        };
        // Layer opacity applies at the stack blend, so the layer source uses
        // the fill-opacity alpha.
        let layer_shape = work.apply_layer_stage(stage);
        union_into(&mut stack_shape, &layer_shape);
        union_into(&mut stack_alpha, &layer_shape);
        // Emboss styles shade the composited layer.
        for emboss in emboss_paints(effects, &shape, context.global_light, options)? {
            apply_exterior(
                &mut work,
                &emboss,
                padded,
                viewport,
                &mut stack_shape,
                &mut stack_alpha,
            );
        }
        self.mix_stack(&work, node.opacity, &stack_shape, &stack_alpha);
        Ok(())
    }

    /// Blend a completed layer-plus-effects stack back at the layer opacity.
    ///
    /// The stack result and the previous backdrop mix with equal weights in
    /// premultiplied space, so one opacity applies to the whole stack. A full
    /// opacity takes the stack state as it is.
    fn mix_stack(&mut self, work: &Self, opacity: f32, stack_shape: &[f32], stack_alpha: &[f32]) {
        let opacity = if opacity.is_finite() {
            opacity.clamp(0.0, 1.0)
        } else {
            1.0
        };
        if opacity >= 1.0 {
            *self = work.clone();
            return;
        }
        for index in 0..self.count {
            self.shape_g[index] = union(self.shape_g[index], stack_shape[index] * opacity);
            self.alpha_g[index] = union(self.alpha_g[index], stack_alpha[index] * opacity);
        }
        for index in 0..self.count {
            let previous = self.alpha[index];
            let stacked = work.alpha[index];
            let out = previous + (stacked - previous) * opacity;
            if out <= 0.0 {
                self.color[index * 3] = 1.0;
                self.color[index * 3 + 1] = 1.0;
                self.color[index * 3 + 2] = 1.0;
                continue;
            }
            for channel in 0..3 {
                let before = self.color[index * 3 + channel] * previous;
                let after = work.color[index * 3 + channel] * stacked;
                self.color[index * 3 + channel] =
                    ((before + (after - before) * opacity) / out).clamp(0.0, 1.0);
            }
            self.alpha[index] = out;
        }
    }

    /// The coverage the effects of one layer paint through.
    ///
    /// An unclipped layer uses its own geometric coverage. A clipped layer uses
    /// the intersection of its own coverage and the base shape, so an exterior
    /// effect stops at the base.
    fn effect_shape(
        &self,
        geom: &[f32],
        padded: Viewport,
        document: &PsdDocument,
        options: &ParseOptions,
    ) -> Result<Coverage, PsdError> {
        let width = usize::try_from(padded.width()).unwrap_or(0);
        let height = usize::try_from(padded.height()).unwrap_or(0);
        let values = match self.clip_base {
            Some(index) => {
                let base = base_geom(index, document, padded, options)?;
                geom.iter().zip(&base).map(|(a, b)| a.min(*b)).collect()
            }
            None => geom.to_vec(),
        };
        Ok(Coverage {
            width,
            height,
            values,
        })
    }

    /// Compute one layer's color, coverage, and alpha over a viewport.
    pub(crate) fn layer_paint(
        node: &LayerNode,
        document: &PsdDocument,
        viewport: Viewport,
        options: &ParseOptions,
        skips: &SkipCount,
    ) -> Result<LayerPaint, PsdError> {
        let Some(index) = node.index else {
            let count = usize::try_from(viewport.width()).unwrap_or(0)
                * usize::try_from(viewport.height()).unwrap_or(0);
            return Ok(LayerPaint {
                color: vec![0.0; count * 3],
                geom: vec![0.0; count],
                shape: vec![0.0; count],
                alpha: vec![0.0; count],
            });
        };
        let read = read_layer(document, index, viewport, options)?;
        let count = read.shape.len();
        let (color, mut geom) = match &node.content {
            LayerContent::SolidColor(rgba) => (solid_color(*rgba, count), vec![rgba[3]; count]),
            LayerContent::Gradient(fill) => {
                Self::render_gradient(fill, viewport, document, options)?
            }
            LayerContent::Pixels | LayerContent::Unsupported | LayerContent::Adjustment(_) => {
                (read.color.clone(), read.shape.clone())
            }
        };
        let color = if node.clips.is_empty() {
            color
        } else {
            Self::clip_color(node, &color, &geom, document, viewport, options, skips)?
        };
        let mask = read.mask.unwrap_or_else(|| vec![1.0f32; count]);
        for (value, factor) in geom.iter_mut().zip(&mask) {
            *value *= *factor;
        }
        let shape: Vec<f32> = geom.iter().map(|value| value * node.fill_opacity).collect();
        let alpha: Vec<f32> = geom
            .iter()
            .map(|value| value * node.opacity * node.fill_opacity)
            .collect();
        Ok(LayerPaint {
            color,
            geom,
            shape,
            alpha,
        })
    }

    /// Build the isolated paint of a group over a viewport.
    pub(crate) fn group_paint(
        node: &LayerNode,
        document: &PsdDocument,
        viewport: Viewport,
        options: &ParseOptions,
        skips: &SkipCount,
    ) -> Result<LayerPaint, PsdError> {
        let width = usize::try_from(viewport.width()).unwrap_or(0);
        let height = usize::try_from(viewport.height()).unwrap_or(0);
        let count = width * height;
        let mut group = Self::new(
            width,
            height,
            vec![1.0f32; count * 3],
            vec![0.0f32; count],
            true,
            skips,
        );
        for child in &node.children {
            group.apply(child, document, viewport, options)?;
        }
        let color = group.corrected_color();
        let mut geom = group.shape_g.clone();
        let mask = match node.index {
            Some(index)
                if document
                    .file()
                    .layers
                    .get(index)
                    .is_some_and(|layer| layer.mask.is_some()) =>
            {
                read_layer(document, index, viewport, options)?.mask
            }
            _ => None,
        };
        if let Some(mask) = mask {
            for (value, factor) in geom.iter_mut().zip(&mask) {
                *value *= *factor;
            }
        }
        let shape: Vec<f32> = geom.iter().map(|value| value * node.fill_opacity).collect();
        let alpha: Vec<f32> = geom
            .iter()
            .map(|value| value * node.opacity * node.fill_opacity)
            .collect();
        Ok(LayerPaint {
            color,
            geom,
            shape,
            alpha,
        })
    }
}

/// The active effects of a layer node, if any.
pub(crate) fn active_effects(node: &LayerNode) -> Option<&Effects> {
    node.effects.as_ref().filter(|effects| effects.is_active())
}

/// The geometric coverage of a base layer over a padded viewport.
///
/// A clipped layer intersects its effect shape with this plane.
fn base_geom(
    index: usize,
    document: &PsdDocument,
    viewport: Viewport,
    options: &ParseOptions,
) -> Result<Vec<f32>, PsdError> {
    let layer = document
        .file()
        .layers
        .get(index)
        .ok_or(PsdError::LayerOutOfRange {
            index,
            count: document.file().layers.len(),
        })?;
    let read = read_layer(document, index, viewport, options)?;
    let count = read.shape.len();
    let mut geom = match &layer.content {
        LayerContent::SolidColor(rgba) => vec![rgba[3]; count],
        LayerContent::Gradient(fill) => {
            Compositor::render_gradient(fill, viewport, document, options)?.1
        }
        LayerContent::Pixels | LayerContent::Unsupported | LayerContent::Adjustment(_) => {
            read.shape.clone()
        }
    };
    if let Some(mask) = read.mask {
        for (value, factor) in geom.iter_mut().zip(&mask) {
            *value *= factor;
        }
    }
    Ok(geom)
}

/// Composite one exterior paint on a working state.
fn apply_exterior(
    work: &mut Compositor,
    paint: &crate::effects_apply::EffectPaint,
    padded: Viewport,
    viewport: Viewport,
    stack_shape: &mut [f32],
    stack_alpha: &mut [f32],
) {
    let cropped = crop_paint(paint, padded, viewport);
    let alpha: Vec<f32> = cropped
        .coverage
        .iter()
        .map(|value| value * cropped.opacity)
        .collect();
    work.apply_source(&cropped.color, &cropped.coverage, &alpha, cropped.blend);
    union_into(stack_shape, &cropped.coverage);
    union_into(stack_alpha, &alpha);
}

/// Union one coverage or alpha plane into another.
fn union_into(plane: &mut [f32], source: &[f32]) {
    for (value, add) in plane.iter_mut().zip(source) {
        *value = union(*value, *add);
    }
}

/// A canvas-space frame for a layer rectangle.
#[allow(
    clippy::cast_precision_loss,
    reason = "layer coordinates are far below the f32 exact range"
)]
fn layer_frame(bounds: LayerBounds) -> Frame {
    (
        bounds.left as f32,
        bounds.top as f32,
        bounds.width() as f32,
        bounds.height() as f32,
    )
}

/// Expand a viewport by the effect margin, bounded by the canvas and the cap.
///
/// Each axis has its own room, so a wide tile still gets the padding that fits
/// on the other axis. A tile larger than the cap minus twice the margin keeps
/// less than the full margin, and the no-seam guarantee then stops at that
/// boundary.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the margin is finite and bounded by the effect size cap"
)]
fn pad_viewport(viewport: Viewport, margin: f32, canvas: (u32, u32)) -> Viewport {
    if !margin.is_finite() || margin <= 0.0 {
        return viewport;
    }
    let room_x = FULL_TIER_MAX_EDGE.saturating_sub(viewport.width()) / 2;
    let room_y = FULL_TIER_MAX_EDGE.saturating_sub(viewport.height()) / 2;
    let margin_x = (margin.ceil() as u32).min(room_x);
    let margin_y = (margin.ceil() as u32).min(room_y);
    if margin_x == 0 && margin_y == 0 {
        return viewport;
    }
    Viewport::new(
        viewport.left.saturating_sub(margin_x),
        viewport.top.saturating_sub(margin_y),
        (viewport.right + margin_x).min(canvas.0),
        (viewport.bottom + margin_y).min(canvas.1),
    )
}
