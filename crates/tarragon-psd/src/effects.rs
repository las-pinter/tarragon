//! Parsed Photoshop layer effects (layer styles).
//!
//! A layer record can carry the object-based effects block (`lfx2`, `lfxs`, or
//! the older `lmfx`) or the legacy effects block (`lrFX`). The parser turns
//! those blocks into [`Effects`]. The compositor paints the enabled items in
//! layer draw order.
//!
//! Angles are degrees counter-clockwise from the positive x axis. Distances
//! and sizes are pixels. Opacity is `0.0..=1.0`.

use crate::blend::BlendMode;
use crate::content::GradientFill;

/// The largest size, distance, or blur an effect may request in pixels.
///
/// A file-controlled value beyond this is clamped so the effect buffers stay
/// bounded, matching the compositor tile budget.
pub const MAX_EFFECT_SIZE: f32 = 512.0;

/// The document global light that the effects may follow.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GlobalLight {
    /// The light angle in degrees.
    pub angle: f32,
    /// The light altitude in degrees.
    pub altitude: f32,
}

impl Default for GlobalLight {
    fn default() -> Self {
        Self {
            angle: 120.0,
            altitude: 30.0,
        }
    }
}

impl GlobalLight {
    /// The light an effect uses: the document light or the effect's own values.
    #[must_use]
    pub(crate) fn resolve(self, use_global: bool, angle: f32, altitude: f32) -> Self {
        if use_global {
            self
        } else {
            Self { angle, altitude }
        }
    }
}

/// The parsed effects of one layer.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Effects {
    /// The master switch. A false value disables every item.
    pub master: bool,
    /// The items in canonical effect order.
    pub items: Vec<Effect>,
}

impl Effects {
    /// True when the layer has at least one enabled item and the master switch.
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.master && self.items.iter().any(Effect::is_enabled)
    }

    /// The enabled items.
    pub fn enabled(&self) -> impl Iterator<Item = &Effect> {
        self.items.iter().filter(|item| item.is_enabled())
    }

    /// The largest pixel distance an enabled item paints outside the layer.
    ///
    /// The compositor pads a tile by this margin before it builds the effect
    /// coverage, so a tile boundary never cuts a shadow or a glow.
    #[must_use]
    pub fn margin(&self) -> f32 {
        if !self.master {
            return 0.0;
        }
        self.enabled().map(Effect::reach).fold(0.0, f32::max)
    }
}

/// One layer effect.
#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    /// A drop shadow behind the layer.
    DropShadow(Shadow),
    /// An inner shadow inside the layer.
    InnerShadow(Shadow),
    /// A glow outside the layer.
    OuterGlow(Glow),
    /// A glow inside the layer.
    InnerGlow(Glow),
    /// A bevel or emboss highlight and shadow pair.
    Bevel(Bevel),
    /// A satin sheen.
    Satin(Satin),
    /// A solid color fill over the layer.
    ColorOverlay(Overlay),
    /// A gradient fill over the layer.
    GradientOverlay(GradientOverlay),
    /// A stroke along the layer edge.
    Stroke(Stroke),
}

impl Effect {
    /// The shared fields of the item. For a bevel, the highlight fields.
    #[must_use]
    pub fn common(&self) -> &EffectCommon {
        match self {
            Self::DropShadow(item) | Self::InnerShadow(item) => &item.common,
            Self::OuterGlow(item) | Self::InnerGlow(item) => &item.common,
            Self::Bevel(item) => &item.highlight,
            Self::Satin(item) => &item.common,
            Self::ColorOverlay(item) => &item.common,
            Self::GradientOverlay(item) => &item.common,
            Self::Stroke(item) => &item.common,
        }
    }

    /// True when the item is enabled.
    #[must_use]
    pub fn is_enabled(&self) -> bool {
        match self {
            Self::Bevel(item) => item.enabled,
            _ => self.common().enabled,
        }
    }

    /// The largest pixel distance the item reads or paints outside the
    /// layer's own rectangle.
    ///
    /// An interior effect is clipped to the shape, but its blur or distance
    /// transform still reads neighbours across a tile boundary. The margin
    /// keeps those reads whole.
    #[must_use]
    pub fn reach(&self) -> f32 {
        match self {
            Self::DropShadow(item) => item.distance.abs() + item.size + 2.0,
            Self::InnerShadow(item) => {
                item.distance.abs() + item.size + item.spread * item.size + 2.0
            }
            Self::OuterGlow(item) | Self::InnerGlow(item) => item.size + 2.0,
            Self::Stroke(item) => item.size + 2.0,
            Self::Bevel(item) => item.size + item.soften + 2.0,
            Self::Satin(item) => item.distance.abs() + item.size + 2.0,
            Self::ColorOverlay(_) | Self::GradientOverlay(_) => 0.0,
        }
        .min(MAX_EFFECT_SIZE)
    }
}

/// The fields every effect shares.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EffectCommon {
    /// True when the effect is enabled.
    pub enabled: bool,
    /// The blend mode.
    pub blend: BlendMode,
    /// The opacity in `0.0..=1.0`.
    pub opacity: f32,
}

impl Default for EffectCommon {
    fn default() -> Self {
        Self {
            enabled: true,
            blend: BlendMode::Normal,
            opacity: 1.0,
        }
    }
}

/// A drop shadow or an inner shadow.
#[derive(Debug, Clone, PartialEq)]
pub struct Shadow {
    /// The shared fields.
    pub common: EffectCommon,
    /// The straight RGB shadow color.
    pub color: [f32; 3],
    /// The light angle in degrees.
    pub angle: f32,
    /// The offset distance in pixels.
    pub distance: f32,
    /// The blur size in pixels.
    pub size: f32,
    /// The spread (drop) or choke (inner) in `0.0..=1.0`.
    pub spread: f32,
    /// True when the effect uses the document global light.
    pub use_global_light: bool,
    /// Drop shadow only: the layer hides the shadow under its own shape.
    pub knocks_out: bool,
    /// The transfer contour.
    pub contour: Contour,
}

/// A glow paint source.
#[derive(Debug, Clone, PartialEq)]
pub enum GlowPaint {
    /// A solid color.
    Color([f32; 3]),
    /// A gradient sampled across the glow strength.
    Gradient(GradientFill),
}

/// The source of an inner glow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlowSource {
    /// The glow starts at the layer edge.
    Edge,
    /// The glow starts at the layer center.
    Center,
}

/// An outer glow or an inner glow.
#[derive(Debug, Clone, PartialEq)]
pub struct Glow {
    /// The shared fields.
    pub common: EffectCommon,
    /// The paint.
    pub paint: GlowPaint,
    /// The blur size in pixels.
    pub size: f32,
    /// The spread (outer) or choke (inner) in `0.0..=1.0`.
    pub spread: f32,
    /// The quality range in `0.0..=1.0`.
    pub range: f32,
    /// The inner glow source.
    pub source: GlowSource,
    /// The transfer contour.
    pub contour: Contour,
}

/// The stroke position.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StrokePosition {
    /// The stroke sits outside the layer edge.
    Outside,
    /// The stroke sits inside the layer edge.
    Inside,
    /// The stroke straddles the layer edge.
    Center,
}

/// The stroke paint.
#[derive(Debug, Clone, PartialEq)]
pub enum StrokePaint {
    /// A solid color.
    Color([f32; 3]),
    /// A gradient.
    Gradient(GradientFill),
    /// A pattern or any other paint the compositor does not draw.
    Unsupported,
}

/// A stroke.
#[derive(Debug, Clone, PartialEq)]
pub struct Stroke {
    /// The shared fields.
    pub common: EffectCommon,
    /// The stroke width in pixels.
    pub size: f32,
    /// The stroke position.
    pub position: StrokePosition,
    /// The paint.
    pub paint: StrokePaint,
}

/// A solid color overlay.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Overlay {
    /// The shared fields.
    pub common: EffectCommon,
    /// The straight RGB color.
    pub color: [f32; 3],
}

/// A gradient overlay.
#[derive(Debug, Clone, PartialEq)]
pub struct GradientOverlay {
    /// The shared fields.
    pub common: EffectCommon,
    /// The gradient.
    pub gradient: GradientFill,
}

/// A satin sheen.
#[derive(Debug, Clone, PartialEq)]
pub struct Satin {
    /// The shared fields.
    pub common: EffectCommon,
    /// The straight RGB color.
    pub color: [f32; 3],
    /// The light angle in degrees.
    pub angle: f32,
    /// The offset distance in pixels.
    pub distance: f32,
    /// The blur size in pixels.
    pub size: f32,
    /// True when the sheen is inverted.
    pub invert: bool,
    /// The transfer contour.
    pub contour: Contour,
}

/// The bevel style.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BevelStyle {
    /// An inner bevel.
    Inner,
    /// An outer bevel.
    Outer,
    /// An emboss.
    Emboss,
    /// A pillow emboss.
    PillowEmboss,
    /// A stroke emboss.
    StrokeEmboss,
}

/// A bevel and emboss effect.
#[derive(Debug, Clone, PartialEq)]
pub struct Bevel {
    /// True when the effect is enabled.
    pub enabled: bool,
    /// The bevel style.
    pub style: BevelStyle,
    /// The depth as a fraction of the size.
    pub depth: f32,
    /// The bevel width in pixels.
    pub size: f32,
    /// The soften blur in pixels.
    pub soften: f32,
    /// The light angle in degrees.
    pub angle: f32,
    /// The light altitude in degrees.
    pub altitude: f32,
    /// True when the effect uses the document global light.
    pub use_global_light: bool,
    /// True when the light points up.
    pub up: bool,
    /// The highlight shared fields.
    pub highlight: EffectCommon,
    /// The straight RGB highlight color.
    pub highlight_color: [f32; 3],
    /// The shadow shared fields.
    pub shadow: EffectCommon,
    /// The straight RGB shadow color.
    pub shadow_color: [f32; 3],
    /// The gloss contour.
    pub gloss: Contour,
}

/// A transfer contour. An empty point list is the identity.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Contour {
    /// The control points as `(input, output)` pairs in `0.0..=1.0`.
    pub points: Vec<(f32, f32)>,
}

impl Contour {
    /// Apply the contour to a coverage value in `0.0..=1.0`.
    #[must_use]
    pub fn apply(&self, value: f32) -> f32 {
        let value = value.clamp(0.0, 1.0);
        if self.points.len() < 2 {
            return value;
        }
        let mut points = self.points.as_slice();
        if let Some((first, rest)) = points.split_first() {
            if value <= first.0 {
                return first.1;
            }
            points = rest;
        }
        let mut previous = self.points[0];
        for point in points {
            if value <= point.0 {
                let span = point.0 - previous.0;
                if span <= 0.0 {
                    return point.1;
                }
                let t = (value - previous.0) / span;
                return (previous.1 + (point.1 - previous.1) * t).clamp(0.0, 1.0);
            }
            previous = *point;
        }
        previous.1.clamp(0.0, 1.0)
    }
}
