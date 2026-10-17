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
//! visibility, opacity, fill opacity, blend mode, masks, clipping masks, and
//! groups.

pub mod adjustments;
pub mod blend;
pub mod composite;
pub mod content;
pub mod decode;
mod descriptor;
pub mod document;
pub mod error;
pub mod limits;
pub mod merged;
pub mod parse;

pub use adjustments::{Adjustment, Curve, CurvePoint, LevelsChannel};
pub use blend::BlendMode;
pub use composite::CompositeRegion;
pub use content::{GradientFill, GradientStyle, LayerContent};
pub use decode::{ChannelRegion, ImageRegion, Viewport};
pub use document::PsdDocument;
pub use error::PsdError;
pub use limits::{Deadline, FULL_TIER_MAX_EDGE, ParseOptions, full_tier_size};
pub use parse::{
    BlendingRanges, ChannelInfo, Clipping, ColorMode, Compression, GlobalMask, LayerBounds,
    LayerMask, LayerRecord, LayerSection, MergedImage, PsdFile, PsdHeader, PsdVersion,
};
