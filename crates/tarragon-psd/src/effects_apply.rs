//! Paint layer effects over a padded coverage plane.
//!
//! Exterior effects (drop shadow, outer glow, outside stroke, outer bevel)
//! paint behind the layer content. Interior effects (gradient and color
//! overlays, satin, inner glow, inner shadow, inside stroke, inner bevel) paint
//! over the layer content and take the layer shape once, at the layer
//! composite. Emboss styles shade the composited layer. The caller crops the
//! padded result to the render viewport.
//!
//! An interior effect is painted through a map relative to the shape, so the
//! shape multiplies the result once. A fully opaque overlay replaces the color
//! of a half-covered edge pixel and keeps its alpha.
//!
//! One file holds every effect paint kernel, so the paint order and the
//! coverage math stay together.

use crate::blend::BlendMode;
use crate::content::GradientFill;
use crate::decode::Viewport;
use crate::effects::{
    Bevel, BevelStyle, Effect, Effects, GlobalLight, Glow, GlowPaint, Stroke, StrokePaint,
    StrokePosition,
};
use crate::effects_bevel::bevel_maps;
use crate::effects_map::{Coverage, glow_map, shadow_map, stroke_bands, stroke_widths};
use crate::error::PsdError;
use crate::limits::ParseOptions;

/// The coverage above which a pixel counts as inside the layer shape.
pub(crate) const INSIDE_EPS: f32 = 0.5 / 255.0;

/// One effect ready to composite over a plane.
#[derive(Debug, Clone)]
pub(crate) struct EffectPaint {
    /// Straight RGB, three `f32` per pixel.
    pub(crate) color: Vec<f32>,
    /// Coverage in `0.0..=1.0`, one `f32` per pixel.
    pub(crate) coverage: Vec<f32>,
    /// The effect blend mode.
    pub(crate) blend: BlendMode,
    /// The effect opacity in `0.0..=1.0`.
    pub(crate) opacity: f32,
}

/// A canvas-space gradient frame: left, top, width, height.
pub(crate) type Frame = (f32, f32, f32, f32);

/// The context a padded plane needs for canvas-space colors.
#[derive(Debug, Clone, Copy)]
pub(crate) struct PaintContext {
    /// The padded viewport the planes cover.
    pub(crate) viewport: Viewport,
    /// The layer rectangle on the canvas.
    pub(crate) layer_bounds: Frame,
    /// The canvas width and height.
    pub(crate) canvas: (u32, u32),
    /// The document global light for the effects that follow it.
    pub(crate) global_light: GlobalLight,
}

impl PaintContext {
    /// The frame of a gradient fill, from its align flag.
    #[allow(
        clippy::cast_precision_loss,
        reason = "canvas dimensions are far below the f32 exact range"
    )]
    pub(crate) fn frame(self, fill: &GradientFill) -> Frame {
        if fill.align {
            self.layer_bounds
        } else {
            (0.0, 0.0, self.canvas.0 as f32, self.canvas.1 as f32)
        }
    }

    /// The frame of a gradient stroke: the layer bounds grown by the outside
    /// pixel span, less one for the pixel centers, so the ramp spans the
    /// stroke's own extent.
    ///
    /// The growth is the stroke's pixel extent: `out_w` rounded up, less one.
    pub(crate) fn stroke_frame(self, out_w: f32) -> Frame {
        let grow = if out_w.is_finite() {
            (out_w.min(crate::effects::MAX_EFFECT_SIZE).ceil() - 1.0).max(0.0)
        } else {
            0.0
        };
        let (left, top, width, height) = self.layer_bounds;
        (
            left - grow,
            top - grow,
            width + 2.0 * grow,
            height + 2.0 * grow,
        )
    }
}

/// Build the exterior effect paints for a padded plane.
///
/// The order matches Photoshop: drop shadows first, then outer glows, then
/// the outside part of a stroke, then an outer bevel. The caller composites
/// them in this order before the layer content.
///
/// # Errors
///
/// Returns a typed error when the deadline passed or the work was cancelled.
pub(crate) fn exterior_paints(
    effects: &Effects,
    shape: &Coverage,
    fill_opacity: f32,
    context: PaintContext,
    options: &ParseOptions,
) -> Result<Vec<EffectPaint>, PsdError> {
    let mut paints = Vec::new();
    let see_through = 1.0 - fill_opacity.clamp(0.0, 1.0);
    for effect in effects.items.iter().rev() {
        if !effect.is_enabled() {
            continue;
        }
        options.check()?;
        if let Effect::DropShadow(shadow) = effect {
            let mut map = shadow_map(shape, shadow, false, context.global_light);
            if shadow.knocks_out {
                knock_out(&mut map, shape, see_through);
            }
            paints.push(solid_paint(
                &map,
                shadow.color,
                shadow.common.blend,
                shadow.common.opacity,
            ));
        }
    }
    for effect in effects.items.iter().rev() {
        if !effect.is_enabled() {
            continue;
        }
        options.check()?;
        if let Effect::OuterGlow(glow) = effect {
            let mut map = glow_map(shape, glow, false);
            knock_out(&mut map, shape, see_through);
            paints.push(glow_paint(&map, glow));
        }
    }
    for effect in effects.items.iter().rev() {
        if !effect.is_enabled() {
            continue;
        }
        options.check()?;
        if let Effect::Stroke(stroke) = effect
            && stroke.position != StrokePosition::Inside
            && let Some(paint) = stroke_paint(stroke, shape, context, options)?
        {
            paints.push(paint);
        }
    }
    for effect in effects.items.iter().rev() {
        if !effect.is_enabled() {
            continue;
        }
        options.check()?;
        if let Effect::Bevel(bevel) = effect
            && bevel.style == BevelStyle::Outer
        {
            paints.extend(bevel_paints(bevel, shape, context.global_light));
        }
    }
    Ok(paints)
}

/// Build the emboss paints, which shade the composited layer.
///
/// The emboss maps cover the inside and the outside of the shape, so they
/// paint after the layer content. The caller composites them last.
///
/// # Errors
///
/// Returns a typed error when the deadline passed or the work was cancelled.
pub(crate) fn emboss_paints(
    effects: &Effects,
    shape: &Coverage,
    light: GlobalLight,
    options: &ParseOptions,
) -> Result<Vec<EffectPaint>, PsdError> {
    let mut paints = Vec::new();
    for effect in effects.items.iter().rev() {
        if !effect.is_enabled() {
            continue;
        }
        options.check()?;
        if let Effect::Bevel(bevel) = effect
            && matches!(bevel.style, BevelStyle::Emboss | BevelStyle::PillowEmboss)
        {
            paints.extend(bevel_paints(bevel, shape, light));
        }
    }
    Ok(paints)
}

/// Build the highlight and shadow paints of a bevel.
fn bevel_paints(bevel: &Bevel, shape: &Coverage, light: GlobalLight) -> [EffectPaint; 2] {
    let (highlight, shadow) = bevel_maps(shape, bevel, light);
    [
        EffectPaint {
            color: solid_color_plane(bevel.highlight_color, highlight.values.len()),
            coverage: highlight.values,
            blend: bevel.highlight.blend,
            opacity: bevel.highlight.opacity,
        },
        EffectPaint {
            color: solid_color_plane(bevel.shadow_color, shadow.values.len()),
            coverage: shadow.values,
            blend: bevel.shadow.blend,
            opacity: bevel.shadow.opacity,
        },
    ]
}

/// Keep an exterior effect behind the layer shape when the fill is see-through.
fn knock_out(map: &mut Coverage, shape: &Coverage, see_through: f32) {
    for (value, alpha) in map.values.iter_mut().zip(&shape.values) {
        *value *= 1.0 - alpha * see_through;
    }
}

/// Build the solid paint of an exterior effect.
fn solid_paint(map: &Coverage, color: [f32; 3], blend: BlendMode, opacity: f32) -> EffectPaint {
    EffectPaint {
        color: solid_color_plane(color, map.values.len()),
        coverage: map.values.clone(),
        blend,
        opacity,
    }
}

/// Build the glow paint, which may sample a gradient across the strength.
fn glow_paint(map: &Coverage, glow: &Glow) -> EffectPaint {
    match &glow.paint {
        GlowPaint::Color(color) => solid_paint(map, *color, glow.common.blend, glow.common.opacity),
        GlowPaint::Gradient(gradient) => {
            let mut plane = vec![0.0f32; map.values.len() * 3];
            for index in 0..map.values.len() {
                let sample = gradient.sample(1.0 - map.values[index].clamp(0.0, 1.0));
                plane[index * 3] = sample[0];
                plane[index * 3 + 1] = sample[1];
                plane[index * 3 + 2] = sample[2];
            }
            EffectPaint {
                color: plane,
                coverage: map.values.clone(),
                blend: glow.common.blend,
                opacity: glow.common.opacity,
            }
        }
    }
}

/// Build the outside band of a stroke as an exterior paint.
fn stroke_paint(
    stroke: &Stroke,
    shape: &Coverage,
    context: PaintContext,
    options: &ParseOptions,
) -> Result<Option<EffectPaint>, PsdError> {
    let (_, outside_width) = stroke_widths(stroke.size, stroke.position);
    let (_, outside) = stroke_bands(shape, stroke.size, stroke.position);
    let frame = context.stroke_frame(outside_width);
    let Some(color) = paint_plane(&outside, &stroke.paint, frame, context, options)? else {
        return Ok(None);
    };
    Ok(Some(EffectPaint {
        color,
        coverage: outside.values,
        blend: stroke.common.blend,
        opacity: stroke.common.opacity,
    }))
}

/// Sample one paint into a color plane over the whole map.
///
/// An unsupported paint returns `None`, so the effect is a no-op.
fn paint_plane(
    map: &Coverage,
    paint: &StrokePaint,
    frame: Frame,
    context: PaintContext,
    options: &ParseOptions,
) -> Result<Option<Vec<f32>>, PsdError> {
    Ok(match paint {
        StrokePaint::Color(color) => Some(solid_color_plane(*color, map.values.len())),
        StrokePaint::Gradient(fill) => Some(paint_gradient_plane(
            map,
            fill,
            frame,
            context.viewport,
            options,
        )?),
        StrokePaint::Unsupported => None,
    })
}

/// A constant RGB plane.
fn solid_color_plane(color: [f32; 3], pixels: usize) -> Vec<f32> {
    let mut plane = vec![0.0f32; pixels * 3];
    for index in 0..pixels {
        plane[index * 3] = color[0];
        plane[index * 3 + 1] = color[1];
        plane[index * 3 + 2] = color[2];
    }
    plane
}

/// A gradient color plane over a padded coverage.
#[allow(
    clippy::cast_precision_loss,
    reason = "plane coordinates are far below the f32 exact range"
)]
fn paint_gradient_plane(
    map: &Coverage,
    fill: &GradientFill,
    frame: Frame,
    viewport: Viewport,
    options: &ParseOptions,
) -> Result<Vec<f32>, PsdError> {
    let mut plane = vec![0.0f32; map.values.len() * 3];
    for y in 0..map.height {
        options.check()?;
        for x in 0..map.width {
            let index = y * map.width + x;
            let sample = fill.sample(fill.parameter_at(
                frame,
                viewport.left as f32 + x as f32 + 0.5,
                viewport.top as f32 + y as f32 + 0.5,
            ));
            plane[index * 3] = sample[0];
            plane[index * 3 + 1] = sample[1];
            plane[index * 3 + 2] = sample[2];
        }
    }
    Ok(plane)
}

/// Crop a padded color plane to a viewport.
#[must_use]
pub(crate) fn crop_color(values: &[f32], padded: Viewport, viewport: Viewport) -> Vec<f32> {
    let padded_width = usize::try_from(padded.width()).unwrap_or(0);
    let width = usize::try_from(viewport.width()).unwrap_or(0);
    let height = usize::try_from(viewport.height()).unwrap_or(0);
    let mut out = vec![0.0f32; width * height * 3];
    for y in 0..height {
        let source_y = usize::try_from(viewport.top - padded.top).unwrap_or(0) + y;
        for x in 0..width {
            let source_x = usize::try_from(viewport.left - padded.left).unwrap_or(0) + x;
            let source = (source_y * padded_width + source_x) * 3;
            let target = (y * width + x) * 3;
            out[target] = values[source];
            out[target + 1] = values[source + 1];
            out[target + 2] = values[source + 2];
        }
    }
    out
}

/// Crop a padded single-channel plane to a viewport.
#[must_use]
pub(crate) fn crop_plane(values: &[f32], padded: Viewport, viewport: Viewport) -> Vec<f32> {
    let padded_width = usize::try_from(padded.width()).unwrap_or(0);
    let width = usize::try_from(viewport.width()).unwrap_or(0);
    let height = usize::try_from(viewport.height()).unwrap_or(0);
    let mut out = vec![0.0f32; width * height];
    for y in 0..height {
        let source_y = usize::try_from(viewport.top - padded.top).unwrap_or(0) + y;
        for x in 0..width {
            let source_x = usize::try_from(viewport.left - padded.left).unwrap_or(0) + x;
            out[y * width + x] = values[source_y * padded_width + source_x];
        }
    }
    out
}

/// Crop an exterior paint to a viewport.
#[must_use]
pub(crate) fn crop_paint(paint: &EffectPaint, padded: Viewport, viewport: Viewport) -> EffectPaint {
    EffectPaint {
        color: crop_color(&paint.color, padded, viewport),
        coverage: crop_plane(&paint.coverage, padded, viewport),
        blend: paint.blend,
        opacity: paint.opacity,
    }
}
