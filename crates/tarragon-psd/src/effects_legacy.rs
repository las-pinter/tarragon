//! Parse the legacy effects block (`lrFX`).
//!
//! The legacy block holds a two-byte version, a two-byte item count, then one
//! `8BIM` keyed item per effect. Sizes, distances, and angles are 16.16 fixed
//! point. Only the six legacy effect kinds exist; later kinds such as satin
//! and strokes live in the object-based block.
//!
//! A short or malformed block returns `None`. A color space the parser cannot
//! read drops that one item and keeps the rest.

use crate::blend::BlendMode;
use crate::effects::{
    Bevel, BevelStyle, Contour, Effect, EffectCommon, Effects, Glow, GlowPaint, GlowSource,
    MAX_EFFECT_SIZE, Overlay, Shadow,
};
use crate::reader::Reader;

/// Parse an `lrFX` block.
#[must_use]
pub(crate) fn parse_legacy_effects(data: &[u8]) -> Option<Effects> {
    let mut reader = Reader::new(data);
    let _version = reader.read_u16().ok()?;
    let count = reader.read_u16().ok()?;
    let mut master = true;
    let mut items = Vec::new();
    for _ in 0..count {
        let signature = reader.read_array::<4>().ok()?;
        if &signature != b"8BIM" {
            return None;
        }
        let key = reader.read_array::<4>().ok()?;
        let length = reader.read_u32().ok()?;
        let block = reader.take(usize::try_from(length).ok()?).ok()?;
        match &key {
            b"cmnS" => {
                if block.len() >= 5 {
                    master = block[4] != 0;
                }
            }
            b"dsdw" => {
                if let Some(effect) = legacy_shadow(block, false) {
                    items.push(effect);
                }
            }
            b"isdw" => {
                if let Some(effect) = legacy_shadow(block, true) {
                    items.push(effect);
                }
            }
            b"oglw" => {
                if let Some(effect) = legacy_glow(block, false) {
                    items.push(effect);
                }
            }
            b"iglw" => {
                if let Some(effect) = legacy_glow(block, true) {
                    items.push(effect);
                }
            }
            b"bevl" => {
                if let Some(effect) = legacy_bevel(block) {
                    items.push(effect);
                }
            }
            b"sofi" => {
                if let Some(effect) = legacy_solid_fill(block) {
                    items.push(effect);
                }
            }
            // A key this parser does not know is skipped, not an error.
            _ => {}
        }
    }
    Some(Effects { master, items })
}

/// Read a 16.16 fixed-point value as pixels.
#[allow(
    clippy::cast_possible_truncation,
    reason = "16.16 pixels fit the f32 range comfortably"
)]
fn fixed(value: u32) -> f32 {
    (f64::from(value) / 65536.0) as f32
}

/// Read a legacy color record: a space code and four 16-bit components.
fn legacy_color(data: &[u8]) -> Option<[f32; 3]> {
    if data.len() < 10 {
        return None;
    }
    let space = u16::from_be_bytes([data[0], data[1]]);
    let mut parts = [0u16; 4];
    for (index, slot) in parts.iter_mut().enumerate() {
        let offset = 2 + index * 2;
        *slot = u16::from_be_bytes([data[offset], data[offset + 1]]);
    }
    let unit = |value: u16| f32::from(value) / 65535.0;
    match space {
        // RGB.
        0 => Some([unit(parts[0]), unit(parts[1]), unit(parts[2])]),
        // HSB.
        1 => {
            let hue = unit(parts[0]) * 360.0;
            let saturation = unit(parts[1]);
            let brightness = unit(parts[2]);
            Some(hsb_to_rgb(hue, saturation, brightness))
        }
        // CMYK.
        2 => {
            let k = unit(parts[3]);
            Some([
                (1.0 - unit(parts[0])) * (1.0 - k),
                (1.0 - unit(parts[1])) * (1.0 - k),
                (1.0 - unit(parts[2])) * (1.0 - k),
            ])
        }
        // Grayscale.
        8 => {
            let gray = unit(parts[0]);
            Some([gray, gray, gray])
        }
        // Lab and the custom spaces are out of scope.
        _ => None,
    }
}

/// Convert HSB to straight RGB. Hue is degrees, saturation and brightness are
/// `0.0..=1.0`.
#[allow(
    clippy::cast_possible_truncation,
    reason = "the hue sector is a small whole number"
)]
fn hsb_to_rgb(hue: f32, saturation: f32, brightness: f32) -> [f32; 3] {
    let hue = hue.rem_euclid(360.0) / 60.0;
    let sector = hue.floor();
    let fraction = hue - sector;
    let p = brightness * (1.0 - saturation);
    let q = brightness * (1.0 - saturation * fraction);
    let t = brightness * (1.0 - saturation * (1.0 - fraction));
    match sector as i32 % 6 {
        0 => [brightness, t, p],
        1 => [q, brightness, p],
        2 => [p, brightness, t],
        3 => [p, q, brightness],
        4 => [t, p, brightness],
        _ => [brightness, p, q],
    }
}

/// Read the shared blend id and opacity after a color record.
fn legacy_common(blend: [u8; 4], opacity: u8) -> EffectCommon {
    EffectCommon {
        enabled: true,
        blend: BlendMode::from_key(blend),
        opacity: f32::from(opacity) / 255.0,
    }
}

/// Parse a legacy drop shadow or inner shadow.
fn legacy_shadow(data: &[u8], inner: bool) -> Option<Effect> {
    let mut reader = Reader::new(data);
    let _version = reader.read_u32().ok()?;
    let blur = fixed(reader.read_u32().ok()?);
    let _intensity = reader.read_u32().ok()?;
    let angle = fixed(reader.read_u32().ok()?);
    let distance = fixed(reader.read_u32().ok()?);
    let color = legacy_color(reader.take(10).ok()?)?;
    let signature = reader.read_array::<4>().ok()?;
    if &signature != b"8BIM" {
        return None;
    }
    let blend = reader.read_array::<4>().ok()?;
    let enabled = reader.read_u8().ok()? != 0;
    let use_global = reader.read_u8().ok()? != 0;
    let opacity = reader.read_u8().ok()?;
    let mut common = legacy_common(blend, opacity);
    common.enabled = enabled;
    let shadow = Shadow {
        common,
        color,
        angle,
        distance: distance.min(MAX_EFFECT_SIZE),
        size: blur.min(MAX_EFFECT_SIZE),
        spread: 0.0,
        use_global_light: use_global,
        knocks_out: !inner,
        contour: Contour::default(),
    };
    Some(if inner {
        Effect::InnerShadow(shadow)
    } else {
        Effect::DropShadow(shadow)
    })
}

/// Parse a legacy outer glow or inner glow.
fn legacy_glow(data: &[u8], inner: bool) -> Option<Effect> {
    let mut reader = Reader::new(data);
    let _version = reader.read_u32().ok()?;
    let blur = fixed(reader.read_u32().ok()?);
    let _intensity = reader.read_u32().ok()?;
    let color = legacy_color(reader.take(10).ok()?)?;
    let signature = reader.read_array::<4>().ok()?;
    if &signature != b"8BIM" {
        return None;
    }
    let blend = reader.read_array::<4>().ok()?;
    let enabled = reader.read_u8().ok()? != 0;
    let opacity = reader.read_u8().ok()?;
    let mut common = legacy_common(blend, opacity);
    common.enabled = enabled;
    let glow = Glow {
        common,
        paint: GlowPaint::Color(color),
        size: blur.min(MAX_EFFECT_SIZE),
        spread: 0.0,
        range: 0.5,
        source: GlowSource::Edge,
        contour: Contour::default(),
    };
    Some(if inner {
        Effect::InnerGlow(glow)
    } else {
        Effect::OuterGlow(glow)
    })
}

/// Parse a legacy bevel.
fn legacy_bevel(data: &[u8]) -> Option<Effect> {
    let mut reader = Reader::new(data);
    let version = reader.read_u32().ok()?;
    let angle = fixed(reader.read_u32().ok()?);
    let depth = fixed(reader.read_u32().ok()?);
    let blur = fixed(reader.read_u32().ok()?);
    let highlight_signature = reader.read_array::<4>().ok()?;
    if &highlight_signature != b"8BIM" {
        return None;
    }
    let highlight_blend = reader.read_array::<4>().ok()?;
    let shadow_signature = reader.read_array::<4>().ok()?;
    if &shadow_signature != b"8BIM" {
        return None;
    }
    let shadow_blend = reader.read_array::<4>().ok()?;
    let highlight_color = legacy_color(reader.take(10).ok()?)?;
    let shadow_color = legacy_color(reader.take(10).ok()?)?;
    let style = reader.read_u8().ok()?;
    let highlight_opacity = reader.read_u8().ok()?;
    let shadow_opacity = reader.read_u8().ok()?;
    let enabled = reader.read_u8().ok()? != 0;
    let use_global = reader.read_u8().ok()? != 0;
    let direction = reader.read_u8().ok()?;
    // Version 2 carries the native colors, which this parser does not use.
    let _ = version;
    Some(Effect::Bevel(Bevel {
        enabled,
        style: match style {
            0 => BevelStyle::Outer,
            2 => BevelStyle::Emboss,
            3 => BevelStyle::PillowEmboss,
            _ => BevelStyle::Inner,
        },
        depth: depth.max(0.0),
        size: blur.min(MAX_EFFECT_SIZE),
        soften: 0.0,
        angle,
        altitude: 30.0,
        use_global_light: use_global,
        up: direction == 0,
        highlight: EffectCommon {
            enabled,
            blend: BlendMode::from_key(highlight_blend),
            opacity: f32::from(highlight_opacity) / 255.0,
        },
        highlight_color,
        shadow: EffectCommon {
            enabled,
            blend: BlendMode::from_key(shadow_blend),
            opacity: f32::from(shadow_opacity) / 255.0,
        },
        shadow_color,
        gloss: Contour::default(),
    }))
}

/// Parse a legacy solid fill.
fn legacy_solid_fill(data: &[u8]) -> Option<Effect> {
    let mut reader = Reader::new(data);
    let _version = reader.read_u32().ok()?;
    let signature = reader.read_array::<4>().ok()?;
    if &signature != b"8BIM" {
        return None;
    }
    let blend = reader.read_array::<4>().ok()?;
    let color = legacy_color(reader.take(10).ok()?)?;
    let opacity = reader.read_u8().ok()?;
    let enabled = reader.read_u8().ok()? != 0;
    let mut common = legacy_common(blend, opacity);
    common.enabled = enabled;
    Some(Effect::ColorOverlay(Overlay { common, color }))
}
