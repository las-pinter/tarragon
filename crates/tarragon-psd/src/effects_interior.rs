//! Paint the interior layer effects over the layer color plane.
//!
//! Interior effects (gradient and color overlays, satin, inner glow, inner
//! shadow, inside stroke, inner bevel) paint over the layer content through a
//! map relative to the shape, so the shape multiplies the result once. The
//! deadline is checked between effect passes and per gradient row.

use crate::blend::{BlendMode, blend_rgb, union};
use crate::content::GradientFill;
use crate::decode::Viewport;
use crate::effects::{
    BevelStyle, Effect, Effects, GlowPaint, GradientOverlay, StrokePaint, StrokePosition,
};
use crate::effects_apply::{Frame, INSIDE_EPS, PaintContext};
use crate::effects_bevel::bevel_maps;
use crate::effects_map::{Coverage, glow_map, satin_map, shadow_map, stroke_bands, stroke_widths};
use crate::error::PsdError;
use crate::limits::ParseOptions;

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
/// The `strength` plane collects the union of the effect coverages, relative
/// to the shape, so the caller can keep the effects visible at a low fill.
///
/// `shape` is the padded geometric shape and `color` the padded layer color.
/// The shape applies once, at the layer composite. The first listed instance
/// of an effect kind is on top, so the paint order is reversed.
#[allow(
    clippy::too_many_lines,
    reason = "one pass per effect kind keeps the paint order explicit"
)]
///
/// # Errors
///
/// Returns a typed error when the deadline passed or the work was cancelled.
pub(crate) fn paint_interior(
    color: &mut [f32],
    strength: &mut [f32],
    effects: &Effects,
    shape: &Coverage,
    context: PaintContext,
    options: &ParseOptions,
) -> Result<(), PsdError> {
    let interior = interior_map(shape);
    // A gradient overlay is painted before a color overlay, so the color
    // overlay is on top.
    for effect in effects.items.iter().rev() {
        if !effect.is_enabled() {
            continue;
        }
        options.check()?;
        if let Effect::GradientOverlay(overlay) = effect {
            paint_gradient_overlay(color, strength, &interior, overlay, context, options)?;
        }
    }
    for effect in effects.items.iter().rev() {
        if !effect.is_enabled() {
            continue;
        }
        options.check()?;
        if let Effect::ColorOverlay(overlay) = effect {
            paint_solid(
                color,
                strength,
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
        options.check()?;
        if let Effect::Satin(satin) = effect {
            let map = relative(&satin_map(shape, satin), shape);
            paint_solid(
                color,
                strength,
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
        options.check()?;
        if let Effect::InnerGlow(glow) = effect {
            let map = relative(&glow_map(shape, glow, true), shape);
            match &glow.paint {
                GlowPaint::Color(rgb) => {
                    paint_solid(
                        color,
                        strength,
                        &map,
                        *rgb,
                        glow.common.blend,
                        glow.common.opacity,
                    );
                }
                GlowPaint::Gradient(fill) => {
                    paint_glow_gradient(
                        color,
                        strength,
                        &map,
                        fill,
                        glow.common.blend,
                        glow.common.opacity,
                    );
                }
            }
        }
    }
    for effect in effects.items.iter().rev() {
        if !effect.is_enabled() {
            continue;
        }
        options.check()?;
        if let Effect::InnerShadow(shadow) = effect {
            let map = relative(
                &shadow_map(shape, shadow, true, context.global_light),
                shape,
            );
            paint_solid(
                color,
                strength,
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
        options.check()?;
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
                    strength,
                    &map,
                    *rgb,
                    stroke.common.blend,
                    stroke.common.opacity,
                ),
                StrokePaint::Gradient(fill) => paint_gradient(
                    color,
                    strength,
                    &map,
                    fill,
                    frame,
                    context.viewport,
                    stroke.common.blend,
                    stroke.common.opacity,
                    options,
                )?,
                StrokePaint::Unsupported => {}
            }
        }
    }
    for effect in effects.items.iter().rev() {
        if !effect.is_enabled() {
            continue;
        }
        options.check()?;
        if let Effect::Bevel(bevel) = effect
            && matches!(bevel.style, BevelStyle::Inner | BevelStyle::StrokeEmboss)
        {
            let (highlight, shadow) = bevel_maps(shape, bevel, context.global_light);
            paint_solid(
                color,
                strength,
                &relative(&highlight, shape),
                bevel.highlight_color,
                bevel.highlight.blend,
                bevel.highlight.opacity,
            );
            paint_solid(
                color,
                strength,
                &relative(&shadow, shape),
                bevel.shadow_color,
                bevel.shadow.blend,
                bevel.shadow.opacity,
            );
        }
    }
    Ok(())
}

/// Paint one solid color through a coverage map.
fn paint_solid(
    color: &mut [f32],
    strength: &mut [f32],
    map: &Coverage,
    rgb: [f32; 3],
    blend: BlendMode,
    opacity: f32,
) {
    for index in 0..map.values.len() {
        let coverage = map.values[index] * opacity;
        if coverage <= 0.0 {
            continue;
        }
        strength[index] = union(strength[index], coverage);
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
    reason = "the paint kernel needs the plane, the map, the sample frame, and the options"
)]
fn paint_gradient(
    color: &mut [f32],
    strength: &mut [f32],
    map: &Coverage,
    fill: &GradientFill,
    frame: Frame,
    viewport: Viewport,
    blend: BlendMode,
    opacity: f32,
    options: &ParseOptions,
) -> Result<(), PsdError> {
    let width = map.width;
    for y in 0..map.height {
        options.check()?;
        for x in 0..width {
            let index = y * width + x;
            let coverage = map.values[index] * opacity;
            if coverage <= 0.0 {
                continue;
            }
            strength[index] = union(strength[index], coverage);
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
    Ok(())
}

/// Paint a gradient glow: the ramp runs along the glow strength, so the
/// first stop hugs the edge.
fn paint_glow_gradient(
    color: &mut [f32],
    strength: &mut [f32],
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
        strength[index] = union(strength[index], coverage);
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
    strength: &mut [f32],
    map: &Coverage,
    overlay: &GradientOverlay,
    context: PaintContext,
    options: &ParseOptions,
) -> Result<(), PsdError> {
    paint_gradient(
        color,
        strength,
        map,
        &overlay.gradient,
        context.frame(&overlay.gradient),
        context.viewport,
        overlay.common.blend,
        overlay.common.opacity,
        options,
    )
}

#[cfg(test)]
mod tests {
    use super::paint_gradient;
    use crate::blend::BlendMode;
    use crate::content::{GradientFill, GradientStyle};
    use crate::decode::Viewport;
    use crate::effects_map::Coverage;
    use crate::error::PsdError;
    use crate::limits::ParseOptions;

    /// A black to white linear ramp.
    fn ramp() -> GradientFill {
        GradientFill {
            stops: vec![(0.0, [0.0, 0.0, 0.0, 1.0]), (1.0, [1.0, 1.0, 1.0, 1.0])],
            midpoints: vec![0.5],
            opacity_stops: Vec::new(),
            angle: 0.0,
            scale: 1.0,
            style: GradientStyle::Linear,
            reverse: false,
            offset: (0.0, 0.0),
            dither: false,
            align: false,
        }
    }

    /// Paint the ramp through a full map with the given options.
    fn paint(options: &ParseOptions) -> Result<Vec<f32>, PsdError> {
        let map = Coverage::new(4, 4, 1.0);
        let mut color = vec![0.0f32; 4 * 4 * 3];
        let mut strength = vec![0.0f32; 4 * 4];
        paint_gradient(
            &mut color,
            &mut strength,
            &map,
            &ramp(),
            (0.0, 0.0, 4.0, 4.0),
            Viewport::new(0, 0, 4, 4),
            BlendMode::Normal,
            1.0,
            options,
        )?;
        Ok(color)
    }

    #[test]
    fn a_gradient_paint_stops_at_an_expired_deadline() {
        assert!(matches!(
            paint(&ParseOptions::in_seconds(0)),
            Err(PsdError::Timeout)
        ));
        let color = paint(&ParseOptions::none()).expect("paints");
        assert!(color.iter().any(|value| *value > 0.0));
    }
}
