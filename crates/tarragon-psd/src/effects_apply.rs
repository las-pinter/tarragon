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

use crate::blend::{BlendMode, blend_rgb};
use crate::content::GradientFill;
use crate::decode::Viewport;
use crate::effects::{
    Bevel, BevelStyle, Effect, Effects, GlobalLight, Glow, GlowPaint, GradientOverlay, Stroke,
    StrokePaint, StrokePosition,
};
use crate::effects_map::{
    Coverage, bevel_maps, glow_map, satin_map, shadow_map, stroke_bands, stroke_widths,
};

/// The coverage above which a pixel counts as inside the layer shape.
const INSIDE_EPS: f32 = 0.5 / 255.0;

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
    fn frame(self, fill: &GradientFill) -> Frame {
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
    fn stroke_frame(self, out_w: f32) -> Frame {
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
#[must_use]
pub(crate) fn exterior_paints(
    effects: &Effects,
    shape: &Coverage,
    fill_opacity: f32,
    context: PaintContext,
) -> Vec<EffectPaint> {
    let mut paints = Vec::new();
    let see_through = 1.0 - fill_opacity.clamp(0.0, 1.0);
    for effect in effects.items.iter().rev() {
        if !effect.is_enabled() {
            continue;
        }
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
        if let Effect::Stroke(stroke) = effect
            && stroke.position != StrokePosition::Inside
            && let Some(paint) = stroke_paint(stroke, shape, context)
        {
            paints.push(paint);
        }
    }
    for effect in effects.items.iter().rev() {
        if !effect.is_enabled() {
            continue;
        }
        if let Effect::Bevel(bevel) = effect
            && bevel.style == BevelStyle::Outer
        {
            paints.extend(bevel_paints(bevel, shape, context.global_light));
        }
    }
    paints
}

/// Build the emboss paints, which shade the composited layer.
///
/// The emboss maps cover the inside and the outside of the shape, so they
/// paint after the layer content. The caller composites them last.
#[must_use]
pub(crate) fn emboss_paints(
    effects: &Effects,
    shape: &Coverage,
    light: GlobalLight,
) -> Vec<EffectPaint> {
    let mut paints = Vec::new();
    for effect in effects.items.iter().rev() {
        if !effect.is_enabled() {
            continue;
        }
        if let Effect::Bevel(bevel) = effect
            && matches!(bevel.style, BevelStyle::Emboss | BevelStyle::PillowEmboss)
        {
            paints.extend(bevel_paints(bevel, shape, light));
        }
    }
    paints
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
fn stroke_paint(stroke: &Stroke, shape: &Coverage, context: PaintContext) -> Option<EffectPaint> {
    let (_, outside_width) = stroke_widths(stroke.size, stroke.position);
    let (_, outside) = stroke_bands(shape, stroke.size, stroke.position);
    let frame = context.stroke_frame(outside_width);
    let color = paint_plane(&outside, &stroke.paint, frame, context)?;
    Some(EffectPaint {
        color,
        coverage: outside.values,
        blend: stroke.common.blend,
        opacity: stroke.common.opacity,
    })
}

/// Sample one paint into a color plane over the whole map.
///
/// An unsupported paint returns `None`, so the effect is a no-op.
fn paint_plane(
    map: &Coverage,
    paint: &StrokePaint,
    frame: Frame,
    context: PaintContext,
) -> Option<Vec<f32>> {
    match paint {
        StrokePaint::Color(color) => Some(solid_color_plane(*color, map.values.len())),
        StrokePaint::Gradient(fill) => {
            Some(paint_gradient_plane(map, fill, frame, context.viewport))
        }
        StrokePaint::Unsupported => None,
    }
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
) -> Vec<f32> {
    let mut plane = vec![0.0f32; map.values.len() * 3];
    for y in 0..map.height {
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
    plane
}

/// A full interior map: one inside the shape, zero outside.
fn interior_map(shape: &Coverage) -> Coverage {
    Coverage {
        width: shape.width,
        height: shape.height,
        values: shape
            .values
            .iter()
            .map(|value| if *value > INSIDE_EPS { 1.0 } else { 0.0 })
            .collect(),
    }
}

/// A map relative to the shape.
///
/// A relative map keeps the paint inside the shape, and the layer composite
/// then applies the shape once. Painting the shape twice blends an
/// anti-aliased edge too little.
fn relative(map: &Coverage, shape: &Coverage) -> Coverage {
    Coverage {
        width: map.width,
        height: map.height,
        values: map
            .values
            .iter()
            .zip(&shape.values)
            .map(|(value, alpha)| {
                if *alpha > INSIDE_EPS {
                    (value / alpha).min(1.0)
                } else {
                    0.0
                }
            })
            .collect(),
    }
}

/// Paint the interior effects over the layer color plane.
///
/// `shape` is the padded geometric shape and `color` the padded layer color.
/// The shape applies once, at the layer composite. The first listed instance
/// of an effect kind is on top, so the paint order is reversed.
#[allow(
    clippy::too_many_lines,
    reason = "one pass per effect kind keeps the paint order explicit"
)]
pub(crate) fn paint_interior(
    color: &mut [f32],
    effects: &Effects,
    shape: &Coverage,
    context: PaintContext,
) {
    let interior = interior_map(shape);
    // A gradient overlay is painted before a color overlay, so the color
    // overlay is on top.
    for effect in effects.items.iter().rev() {
        if !effect.is_enabled() {
            continue;
        }
        if let Effect::GradientOverlay(overlay) = effect {
            paint_gradient_overlay(color, &interior, overlay, context);
        }
    }
    for effect in effects.items.iter().rev() {
        if !effect.is_enabled() {
            continue;
        }
        if let Effect::ColorOverlay(overlay) = effect {
            paint_solid(
                color,
                &interior,
                overlay.color,
                overlay.common.blend,
                overlay.common.opacity,
            );
        }
    }
    for effect in effects.items.iter().rev() {
        if !effect.is_enabled() {
            continue;
        }
        if let Effect::Satin(satin) = effect {
            let map = relative(&satin_map(shape, satin), shape);
            paint_solid(
                color,
                &map,
                satin.color,
                satin.common.blend,
                satin.common.opacity,
            );
        }
    }
    for effect in effects.items.iter().rev() {
        if !effect.is_enabled() {
            continue;
        }
        if let Effect::InnerGlow(glow) = effect {
            let map = relative(&glow_map(shape, glow, true), shape);
            match &glow.paint {
                GlowPaint::Color(rgb) => {
                    paint_solid(color, &map, *rgb, glow.common.blend, glow.common.opacity);
                }
                GlowPaint::Gradient(fill) => {
                    paint_glow_gradient(color, &map, fill, glow.common.blend, glow.common.opacity);
                }
            }
        }
    }
    for effect in effects.items.iter().rev() {
        if !effect.is_enabled() {
            continue;
        }
        if let Effect::InnerShadow(shadow) = effect {
            let map = relative(
                &shadow_map(shape, shadow, true, context.global_light),
                shape,
            );
            paint_solid(
                color,
                &map,
                shadow.color,
                shadow.common.blend,
                shadow.common.opacity,
            );
        }
    }
    // An inside stroke is painted before an inner bevel, so the bevel is on
    // top.
    for effect in effects.items.iter().rev() {
        if !effect.is_enabled() {
            continue;
        }
        if let Effect::Stroke(stroke) = effect
            && stroke.position != StrokePosition::Outside
        {
            let (inside, _) = stroke_bands(shape, stroke.size, stroke.position);
            let map = relative(&inside, shape);
            let (_, outside_width) = stroke_widths(stroke.size, stroke.position);
            let frame = context.stroke_frame(outside_width);
            match &stroke.paint {
                StrokePaint::Color(rgb) => paint_solid(
                    color,
                    &map,
                    *rgb,
                    stroke.common.blend,
                    stroke.common.opacity,
                ),
                StrokePaint::Gradient(fill) => paint_gradient(
                    color,
                    &map,
                    fill,
                    frame,
                    context.viewport,
                    stroke.common.blend,
                    stroke.common.opacity,
                ),
                StrokePaint::Unsupported => {}
            }
        }
    }
    for effect in effects.items.iter().rev() {
        if !effect.is_enabled() {
            continue;
        }
        if let Effect::Bevel(bevel) = effect
            && matches!(bevel.style, BevelStyle::Inner | BevelStyle::StrokeEmboss)
        {
            let (highlight, shadow) = bevel_maps(shape, bevel, context.global_light);
            paint_solid(
                color,
                &relative(&highlight, shape),
                bevel.highlight_color,
                bevel.highlight.blend,
                bevel.highlight.opacity,
            );
            paint_solid(
                color,
                &relative(&shadow, shape),
                bevel.shadow_color,
                bevel.shadow.blend,
                bevel.shadow.opacity,
            );
        }
    }
}

/// Paint one solid color through a coverage map.
fn paint_solid(color: &mut [f32], map: &Coverage, rgb: [f32; 3], blend: BlendMode, opacity: f32) {
    for index in 0..map.values.len() {
        let coverage = map.values[index] * opacity;
        if coverage <= 0.0 {
            continue;
        }
        let backdrop = [color[index * 3], color[index * 3 + 1], color[index * 3 + 2]];
        let blended = blend_rgb(blend, backdrop, rgb);
        for channel in 0..3 {
            color[index * 3 + channel] =
                backdrop[channel] + (blended[channel] - backdrop[channel]) * coverage;
        }
    }
}

/// Paint a gradient through a coverage map.
#[allow(
    clippy::too_many_arguments,
    clippy::cast_precision_loss,
    reason = "the paint kernel needs the plane, the map, and the sample frame"
)]
fn paint_gradient(
    color: &mut [f32],
    map: &Coverage,
    fill: &GradientFill,
    frame: Frame,
    viewport: Viewport,
    blend: BlendMode,
    opacity: f32,
) {
    let width = map.width;
    for y in 0..map.height {
        for x in 0..width {
            let index = y * width + x;
            let coverage = map.values[index] * opacity;
            if coverage <= 0.0 {
                continue;
            }
            let sample = fill.sample(fill.parameter_at(
                frame,
                viewport.left as f32 + x as f32 + 0.5,
                viewport.top as f32 + y as f32 + 0.5,
            ));
            let rgb = [sample[0], sample[1], sample[2]];
            let backdrop = [color[index * 3], color[index * 3 + 1], color[index * 3 + 2]];
            let blended = blend_rgb(blend, backdrop, rgb);
            for channel in 0..3 {
                color[index * 3 + channel] =
                    backdrop[channel] + (blended[channel] - backdrop[channel]) * coverage;
            }
        }
    }
}

/// Paint a gradient glow: the ramp runs along the glow strength, so the
/// first stop hugs the edge.
fn paint_glow_gradient(
    color: &mut [f32],
    map: &Coverage,
    fill: &GradientFill,
    blend: BlendMode,
    opacity: f32,
) {
    for index in 0..map.values.len() {
        let coverage = map.values[index] * opacity;
        if coverage <= 0.0 {
            continue;
        }
        let sample = fill.sample(1.0 - map.values[index].clamp(0.0, 1.0));
        let rgb = [sample[0], sample[1], sample[2]];
        let backdrop = [color[index * 3], color[index * 3 + 1], color[index * 3 + 2]];
        let blended = blend_rgb(blend, backdrop, rgb);
        for channel in 0..3 {
            color[index * 3 + channel] =
                backdrop[channel] + (blended[channel] - backdrop[channel]) * coverage;
        }
    }
}

/// Paint a gradient overlay over the whole layer shape.
fn paint_gradient_overlay(
    color: &mut [f32],
    map: &Coverage,
    overlay: &GradientOverlay,
    context: PaintContext,
) {
    paint_gradient(
        color,
        map,
        &overlay.gradient,
        context.frame(&overlay.gradient),
        context.viewport,
        overlay.common.blend,
        overlay.common.opacity,
    );
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
