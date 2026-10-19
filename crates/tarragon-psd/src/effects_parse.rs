//! Parse the object-based layer effects block (`lfx2`, `lfxs`, and `lmfx`).
//!
//! The block holds a four-byte version, a four-byte descriptor version, then
//! one Adobe `ActionDescriptor`. Each effect is a nested descriptor keyed by
//! its class id (`DrSh`, `IrSh`, `OrGl`, `IrGl`, `ebbl`, `ChFX`, `SoFi`,
//! `GrFl`, `FrFX`) or by a multi-instance list key.
//!
//! A missing or malformed block returns `None`. A missing or unknown field
//! takes its Photoshop default, and a color the parser cannot read leaves the
//! effect with black paint instead of a panic.

#![allow(
    clippy::cast_possible_truncation,
    reason = "descriptor numbers are f64 but effect fields are small f32 values"
)]

use crate::blend::BlendMode;
use crate::content::{color_from_descriptor, gradient_from_descriptor};
use crate::descriptor::{Descriptor, Value, parse_versioned};
use crate::effects::{
    Bevel, BevelStyle, Contour, Effect, EffectCommon, Effects, Glow, GlowPaint, GlowSource,
    GradientOverlay, MAX_EFFECT_SIZE, Overlay, Satin, Shadow, Stroke, StrokePaint, StrokePosition,
};

/// The single-instance key and the multi-instance list key of each effect.
const KINDS: [(&[u8], &[u8]); 10] = [
    (b"DrSh", b"dropShadowMulti"),
    (b"IrSh", b"innerShadowMulti"),
    (b"OrGl", b""),
    (b"IrGl", b""),
    (b"ebbl", b""),
    (b"ChFX", b""),
    (b"SoFi", b"solidFillMulti"),
    (b"GrFl", b"gradientFillMulti"),
    (b"patternFill", b""),
    (b"FrFX", b"frameFXMulti"),
];

/// Parse an `lfx2`, `lfxs`, or `lmfx` block.
#[must_use]
pub(crate) fn parse_object_effects(data: &[u8]) -> Option<Effects> {
    // The outer version field is not the descriptor version.
    let rest = data.get(4..)?;
    let (descriptor, _) = parse_versioned(rest)?;
    let master = !matches!(
        descriptor.get(b"masterFXSwitch"),
        Some(Value::Boolean(false))
    );
    let mut items = Vec::new();
    for (single, multi) in KINDS {
        let mut found = false;
        if !multi.is_empty()
            && let Some(Value::List(list)) = descriptor.get(multi)
        {
            for value in list {
                if let Value::Descriptor(item) = value
                    && let Some(effect) = parse_one(single, item)
                {
                    items.push(effect);
                    found = true;
                }
            }
        }
        if !found
            && let Some(item) = descriptor.descriptor(single)
            && let Some(effect) = parse_one(single, item)
        {
            items.push(effect);
        }
    }
    Some(Effects { master, items })
}

/// Parse one effect descriptor. A false `present` field drops the effect.
fn parse_one(key: &[u8], item: &Descriptor) -> Option<Effect> {
    if matches!(item.get(b"present"), Some(Value::Boolean(false))) {
        return None;
    }
    Some(match key {
        b"DrSh" => Effect::DropShadow(parse_shadow(item, false)),
        b"IrSh" => Effect::InnerShadow(parse_shadow(item, true)),
        b"OrGl" => Effect::OuterGlow(parse_glow(item, false)),
        b"IrGl" => Effect::InnerGlow(parse_glow(item, true)),
        b"ebbl" => Effect::Bevel(parse_bevel(item)),
        b"ChFX" => Effect::Satin(parse_satin(item)),
        b"SoFi" => Effect::ColorOverlay(Overlay {
            common: common(item, BlendMode::Normal, 1.0),
            color: color(item, b"Clr "),
        }),
        b"GrFl" => Effect::GradientOverlay(GradientOverlay {
            common: common(item, BlendMode::Normal, 1.0),
            gradient: gradient_from_descriptor(item),
        }),
        b"FrFX" => Effect::Stroke(parse_stroke(item)),
        // A pattern overlay needs the document pattern table.
        _ => return None,
    })
}

/// Read the shared enabled, blend, and opacity fields.
fn common(item: &Descriptor, blend: BlendMode, opacity: f32) -> EffectCommon {
    EffectCommon {
        enabled: !matches!(item.get(b"enab"), Some(Value::Boolean(false))),
        blend: blend_of(item, b"Md  ").unwrap_or(blend),
        opacity: percent(item, b"Opct", opacity),
    }
}

/// Map the four-byte effect blend id to a [`BlendMode`].
fn blend_of(item: &Descriptor, key: &[u8]) -> Option<BlendMode> {
    let id = item.enumeration(key)?;
    Some(match id {
        b"Nrml" => BlendMode::Normal,
        b"Dslv" => BlendMode::Dissolve,
        b"Drkn" => BlendMode::Darken,
        b"Mltp" => BlendMode::Multiply,
        b"CBrn" => BlendMode::ColorBurn,
        b"linearBurn" => BlendMode::LinearBurn,
        b"darkerColor" => BlendMode::DarkerColor,
        b"Lghn" => BlendMode::Lighten,
        b"Scrn" => BlendMode::Screen,
        b"CDdg" => BlendMode::ColorDodge,
        b"linearDodge" => BlendMode::LinearDodge,
        b"lighterColor" => BlendMode::LighterColor,
        b"Ovrl" => BlendMode::Overlay,
        b"SftL" => BlendMode::SoftLight,
        b"HrdL" => BlendMode::HardLight,
        b"vividLight" => BlendMode::VividLight,
        b"linearLight" => BlendMode::LinearLight,
        b"pinLight" => BlendMode::PinLight,
        b"hardMix" => BlendMode::HardMix,
        b"Dfrn" => BlendMode::Difference,
        b"Xclu" => BlendMode::Exclusion,
        b"blendSubtraction" => BlendMode::Subtract,
        b"blendDivide" => BlendMode::Divide,
        b"H   " => BlendMode::Hue,
        b"Strt" => BlendMode::Saturation,
        b"Clr " => BlendMode::Color,
        b"Lmns" => BlendMode::Luminosity,
        b"passThrough" => BlendMode::PassThrough,
        _ => return None,
    })
}

/// Read a percentage field as a `0.0..=1.0` fraction.
fn percent(item: &Descriptor, key: &[u8], default: f32) -> f32 {
    let value = item.number(key).unwrap_or(f64::from(default) * 100.0) / 100.0;
    finite(value as f32).clamp(0.0, 1.0)
}

/// Read a pixel field, clamped to the effect size budget.
fn pixels(item: &Descriptor, key: &[u8], default: f32) -> f32 {
    finite(item.number(key).unwrap_or(f64::from(default)) as f32).clamp(0.0, MAX_EFFECT_SIZE)
}

/// Read an angle in degrees.
fn angle(item: &Descriptor, key: &[u8], default: f32) -> f32 {
    finite(item.number(key).unwrap_or(f64::from(default)) as f32)
}

/// Read a boolean field, with a default.
fn boolean(item: &Descriptor, key: &[u8], default: bool) -> bool {
    item.boolean(key).unwrap_or(default)
}

/// Read a color field, or black.
fn color(item: &Descriptor, key: &[u8]) -> [f32; 3] {
    item.descriptor(key)
        .and_then(color_from_descriptor)
        .unwrap_or([0.0, 0.0, 0.0])
}

/// Read a transfer contour. A missing or linear contour is the identity.
fn contour(item: &Descriptor, key: &[u8]) -> Contour {
    let Some(value) = item.descriptor(key) else {
        return Contour::default();
    };
    let Some(Value::List(points)) = value.get(b"Crv ") else {
        return Contour::default();
    };
    let mut parsed: Vec<(f32, f32)> = points
        .iter()
        .filter_map(|point| {
            let Value::Descriptor(point) = point else {
                return None;
            };
            let x = point.number(b"Hrzn")? as f32 / 255.0;
            let y = point.number(b"Vrtc")? as f32 / 255.0;
            Some((x.clamp(0.0, 1.0), y.clamp(0.0, 1.0)))
        })
        .collect();
    parsed.sort_by(|a, b| a.0.total_cmp(&b.0));
    Contour { points: parsed }
}

/// Parse a shadow descriptor.
fn parse_shadow(item: &Descriptor, inner: bool) -> Shadow {
    Shadow {
        common: common(item, BlendMode::Multiply, 0.75),
        color: color(item, b"Clr "),
        angle: angle(item, b"lagl", 120.0),
        distance: pixels(item, b"Dstn", 5.0),
        size: pixels(item, b"blur", 5.0),
        spread: percent(item, b"Ckmt", 0.0),
        use_global_light: boolean(item, b"uglg", true),
        knocks_out: !inner && boolean(item, b"layerConceals", true),
        contour: contour(item, b"TrnS"),
    }
}

/// Parse a glow descriptor.
fn parse_glow(item: &Descriptor, inner: bool) -> Glow {
    let paint = if item.descriptor(b"Grad").is_some() {
        GlowPaint::Gradient(gradient_from_descriptor(item))
    } else {
        GlowPaint::Color(color(item, b"Clr "))
    };
    Glow {
        common: common(item, BlendMode::Screen, 0.75),
        paint,
        size: pixels(item, b"blur", 5.0),
        spread: percent(item, b"Ckmt", 0.0),
        range: percent(item, b"Inpr", 0.5),
        source: if inner && item.enumeration(b"glwS") == Some(b"SrcC") {
            GlowSource::Center
        } else {
            GlowSource::Edge
        },
        contour: contour(item, b"TrnS"),
    }
}

/// Parse a stroke descriptor.
fn parse_stroke(item: &Descriptor) -> Stroke {
    let paint = match item.enumeration(b"PntT") {
        Some(b"GrFl") => StrokePaint::Gradient(gradient_from_descriptor(item)),
        Some(b"Ptrn") => StrokePaint::Unsupported,
        _ => StrokePaint::Color(color(item, b"Clr ")),
    };
    Stroke {
        common: common(item, BlendMode::Normal, 1.0),
        size: pixels(item, b"Sz  ", 3.0),
        position: match item.enumeration(b"Styl") {
            Some(b"InsF") => StrokePosition::Inside,
            Some(b"CtrF") => StrokePosition::Center,
            _ => StrokePosition::Outside,
        },
        paint,
    }
}

/// Parse a satin descriptor.
fn parse_satin(item: &Descriptor) -> Satin {
    Satin {
        common: common(item, BlendMode::Multiply, 0.5),
        color: color(item, b"Clr "),
        angle: angle(item, b"lagl", 19.0),
        distance: pixels(item, b"Dstn", 11.0),
        size: pixels(item, b"blur", 14.0),
        invert: boolean(item, b"Invr", false),
        contour: contour(item, b"MpgS"),
    }
}

/// Parse a bevel and emboss descriptor.
fn parse_bevel(item: &Descriptor) -> Bevel {
    Bevel {
        enabled: !matches!(item.get(b"enab"), Some(Value::Boolean(false))),
        style: match item.enumeration(b"bvlS") {
            Some(b"OtrB") => BevelStyle::Outer,
            Some(b"Embs") => BevelStyle::Emboss,
            Some(b"PlEb") => BevelStyle::PillowEmboss,
            Some(b"strokeEmboss") => BevelStyle::StrokeEmboss,
            _ => BevelStyle::Inner,
        },
        depth: percent(item, b"srgR", 1.0),
        size: pixels(item, b"blur", 5.0),
        soften: pixels(item, b"Sftn", 0.0),
        angle: angle(item, b"lagl", 120.0),
        altitude: angle(item, b"Lald", 30.0),
        use_global_light: boolean(item, b"uglg", true),
        up: item.enumeration(b"bvlD") != Some(b"Out "),
        highlight: EffectCommon {
            enabled: true,
            blend: blend_of(item, b"hglM").unwrap_or(BlendMode::Screen),
            opacity: percent(item, b"hglO", 0.75),
        },
        highlight_color: item
            .descriptor(b"hglC")
            .and_then(color_from_descriptor)
            .unwrap_or([1.0, 1.0, 1.0]),
        shadow: EffectCommon {
            enabled: true,
            blend: blend_of(item, b"sdwM").unwrap_or(BlendMode::Multiply),
            opacity: percent(item, b"sdwO", 0.75),
        },
        shadow_color: color(item, b"sdwC"),
        gloss: contour(item, b"TrnS"),
    }
}

/// Replace a NaN with zero so a bad value cannot poison a buffer.
fn finite(value: f32) -> f32 {
    if value.is_finite() { value } else { 0.0 }
}
