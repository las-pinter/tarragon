//! PSD and PSB parsing, merged preview, and layer compositor.
//!
//! This crate is pure CPU work. It does not depend on the GUI or the database.
//!
//! The parser reads the Photoshop binary format directly. It does not use a
//! `psd` crate. The parser reads the full layer-and-mask section and the
//! merged image data section. A region read decodes only a viewport. The
//! 8192 px full-tier cap and the 120 s deadline live in [`limits`].
//!
//! The compositor reads layers in order over one viewport at a time. It honors
//! visibility, opacity, fill opacity, blend mode, masks, clipping masks,
//! groups, and layer effects.

mod adjustment_parse;
pub mod adjustments;
pub mod blend;
pub mod composite;
mod composite_budget;
mod composite_fill;
mod composite_interior;
mod composite_paint;
mod composite_region;
pub mod content;
pub mod decode;
mod descriptor;
pub mod document;
pub mod effects;
mod effects_apply;
mod effects_bevel;
mod effects_blur;
mod effects_interior;
mod effects_legacy;
mod effects_map;
mod effects_paints;
mod effects_parse;
pub mod error;
mod layer;
mod layer_blocks;
mod layer_extra;
mod layer_source;
mod layer_tree;
pub mod limits;
pub mod merged;
mod model;
pub mod parse;
mod reader;
mod resources;

pub use adjustments::{Adjustment, Curve, CurvePoint, LevelsChannel};
pub use blend::BlendMode;
pub use composite::CompositeRegion;
pub use content::{GradientFill, GradientStyle, LayerContent};
pub use decode::{ChannelRegion, ImageRegion, Viewport};
pub use document::PsdDocument;
pub use effects::{
    Bevel, BevelStyle, Contour, Effect, EffectCommon, Effects, GlobalLight, Glow, GlowPaint,
    GlowSource, GradientOverlay, Overlay, Satin, Shadow, Stroke, StrokePaint, StrokePosition,
};
pub use error::PsdError;
pub use layer::{
    BlendingRanges, ChannelInfo, Clipping, GlobalMask, LayerBounds, LayerMask, LayerRecord,
    LayerSection,
};
pub use limits::{Deadline, FULL_TIER_MAX_EDGE, ParseOptions, full_tier_size};
pub use merged::MergedImage;
pub use model::{ColorMode, Compression, PsdFile, PsdHeader, PsdVersion};
