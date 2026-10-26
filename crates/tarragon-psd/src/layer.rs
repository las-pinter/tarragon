//! Layer records and the layer-and-mask section.

use crate::content::LayerContent;
use crate::effects::Effects;
use crate::error::PsdError;
use crate::layer_extra::read_layer_extra;
use crate::layer_info::parse_layer_info;
use crate::layer_tail::{
    MAX_PATTERN_BLOCK, TaggedTail, drop_unmatched_pattern_fills, read_tagged_tail,
};
use crate::limits::{MAX_DIMENSION, MAX_LAYER_CHANNELS, ParseOptions};
use crate::model::{PsdHeader, sample_levels};
use crate::patterns::PatternEntry;
use crate::reader::{Reader, to_usize};
use crate::source::Source;
use crate::vector_mask::VectorMask;
use crate::window::Window;

/// The blend mode signature in a layer record.
pub(crate) const BLEND_SIGNATURE: &[u8; 4] = b"8BIM";
/// The bytes of the global layer mask that carry values.
const GLOBAL_MASK_BYTES: usize = 13;

/// The clipping flag of a layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Clipping {
    /// The layer is a base layer.
    Base,
    /// The layer clips to the base layer below it.
    NonBase,
}

/// The layer section divider kind that marks group boundaries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayerSection {
    /// Any other section.
    Other,
    /// The open-folder record that closes a group.
    OpenFolder,
    /// The closed-folder record that closes a group.
    ClosedFolder,
    /// The bounding record that opens a group.
    BoundingSectionDivider,
    /// Any other divider code.
    Unknown(u32),
}

impl LayerSection {
    /// Map a divider code to a [`LayerSection`].
    #[must_use]
    pub const fn from_code(code: u32) -> Self {
        match code {
            0 => Self::Other,
            1 => Self::OpenFolder,
            2 => Self::ClosedFolder,
            3 => Self::BoundingSectionDivider,
            other => Self::Unknown(other),
        }
    }

    /// Return true when the record opens a group.
    #[must_use]
    pub const fn opens_group(self) -> bool {
        matches!(self, Self::BoundingSectionDivider)
    }

    /// Return true when the record closes a group.
    #[must_use]
    pub const fn closes_group(self) -> bool {
        matches!(self, Self::OpenFolder | Self::ClosedFolder)
    }
}

/// A layer rectangle in pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LayerBounds {
    /// The top edge.
    pub top: i32,
    /// The left edge.
    pub left: i32,
    /// The bottom edge.
    pub bottom: i32,
    /// The right edge.
    pub right: i32,
}

impl LayerBounds {
    /// The width in pixels. It is negative when `right` is left of `left`.
    #[must_use]
    pub const fn width(self) -> i64 {
        self.right as i64 - self.left as i64
    }

    /// The height in pixels. It is negative when `bottom` is above `top`.
    #[must_use]
    pub const fn height(self) -> i64 {
        self.bottom as i64 - self.top as i64
    }
}

/// Validate a layer rectangle against the Adobe limit.
///
/// The row buffer of a tile decode is sized from the layer width, so a huge
/// file-controlled rectangle must be refused before any allocation.
pub(crate) fn validate_layer_bounds(bounds: LayerBounds) -> Result<(), PsdError> {
    let limit = i64::from(MAX_DIMENSION);
    if bounds.width() > limit || bounds.height() > limit {
        return Err(PsdError::BadDimensions {
            width: u32::try_from(bounds.width()).unwrap_or(u32::MAX),
            height: u32::try_from(bounds.height()).unwrap_or(u32::MAX),
            limit: MAX_DIMENSION,
        });
    }
    Ok(())
}

/// One channel of a layer record or of the merged image.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChannelInfo {
    /// The channel ID. 0 to 3 are color, -1 is alpha, -2 is a user mask.
    pub id: i16,
    /// The declared channel block length in bytes.
    pub length: u64,
    /// The start of the channel block in the file.
    ///
    /// A layer channel points at the two-byte compression field. A merged
    /// channel points at the compressed data after the shared row table.
    pub data_offset: u64,
    /// The start of the compressed sample data in the file.
    pub data_start: u64,
    /// The bytes available for decode after the compression field and the
    /// RLE row-length table. A read past this length is refused.
    pub data_len: u64,
    /// The start of the RLE row-length table, or zero for raw data.
    pub row_table_offset: u64,
    /// The channel compression code.
    pub compression: u16,
    /// The rectangle that the channel covers.
    pub bounds: LayerBounds,
}

/// The layer blending ranges.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlendingRanges {
    /// The composite gray blend: two source and two destination values.
    pub composite: [u16; 4],
    /// The per channel ranges, four values each.
    pub channels: Vec<[u16; 4]>,
}

/// A layer mask rectangle and its flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LayerMask {
    /// The mask rectangle.
    pub bounds: LayerBounds,
    /// The default color, 0 or 255.
    pub default_color: u8,
    /// The mask flag bits.
    pub flags: u8,
    /// The real user mask rectangle, when present.
    pub real_bounds: Option<LayerBounds>,
}

/// The global layer mask info section.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GlobalMask {
    /// The overlay color space code.
    pub overlay_color_space: u16,
    /// The opacity, 0 to 100.
    pub opacity: u16,
    /// The mask kind.
    pub kind: u8,
}

/// One parsed layer record.
#[derive(Debug, Clone, PartialEq)]
pub struct LayerRecord {
    /// The layer name. The unicode name wins over the ASCII name.
    pub name: String,
    /// The layer rectangle.
    pub bounds: LayerBounds,
    /// The layer channels, in file order.
    pub channels: Vec<ChannelInfo>,
    /// The four blend-mode key bytes, for example `norm`.
    pub blend_key: [u8; 4],
    /// The opacity, 0 to 255.
    pub opacity: u8,
    /// The fill opacity, 0 to 255. Defaults to 255 when the block is absent.
    pub fill_opacity: u8,
    /// The clipping flag.
    pub clipping: Clipping,
    /// The raw layer flag bits.
    pub flags: u8,
    /// True when the layer is visible. Bit 1 of the flags clears it.
    pub visible: bool,
    /// The layer mask, when the record carries one.
    pub mask: Option<LayerMask>,
    /// The blending ranges.
    pub blending_ranges: BlendingRanges,
    /// The layer section divider, when the record carries one.
    pub section: Option<LayerSection>,
    /// The parsed layer content: pixels, a fill, or an adjustment.
    pub content: LayerContent,
    /// True when the record carries a vector mask path block.
    pub has_vector_mask: bool,
    /// The parsed vector mask path, when the block is present and valid.
    pub vector_mask: Option<VectorMask>,
    /// The parsed layer effects, when the record carries an effects block.
    pub effects: Option<Effects>,
}

/// The layer-and-mask section: the layer records and the global mask.
pub(crate) struct LayerSectionData {
    /// The declared number of layer records.
    pub(crate) layer_count: usize,
    /// The layer records, in file order.
    pub(crate) layers: Vec<LayerRecord>,
    /// The global layer mask, when present.
    pub(crate) global_mask: Option<GlobalMask>,
    /// The document pattern table.
    pub(crate) patterns: Vec<PatternEntry>,
    /// True when the layer count was negative, which marks merged transparency.
    pub(crate) merged_has_alpha: bool,
    /// The message of the error that the tagged layer info scan dropped.
    pub(crate) layer_info_error: Option<String>,
    /// Why a pattern table was skipped, when it was too large.
    pub(crate) pattern_note: Option<String>,
}

/// The parsed layer info: the records and the sign of the layer count.
pub(crate) struct LayerInfo {
    /// The declared number of layer records.
    pub(crate) layer_count: usize,
    /// The layer records, in file order.
    pub(crate) layers: Vec<LayerRecord>,
    /// True when the layer count was negative.
    pub(crate) merged_has_alpha: bool,
}

/// Parse the layer-and-mask section, the file range `[start, end)`.
///
/// The records are read into memory one at a time. The channel pixels stay in
/// the file and are found later by offset.
pub(crate) fn parse_layer_section(
    source: &Source,
    start: u64,
    end: u64,
    header: &PsdHeader,
    options: &ParseOptions,
) -> Result<LayerSectionData, PsdError> {
    let mut section = Window::new(source, start, end);
    let layer_info_length = section.read_length(header.version)?;
    let mut info = if layer_info_length == 0 {
        LayerInfo {
            layer_count: 0,
            layers: Vec::new(),
            merged_has_alpha: false,
        }
    } else {
        // The layer info length bounds the record and channel walk.
        to_usize(layer_info_length)?;
        section.require(layer_info_length)?;
        let body_start = section.pos();
        let body_end = body_start + layer_info_length;
        let info = parse_layer_info(source, body_start, body_end, header, options)?;
        section.seek(body_end);
        info
    };
    let global_mask = parse_global_mask(&mut section)?;
    // The tagged blocks are optional extras. A bad block means no layers or
    // patterns are found there, and the merged image still renders. Only a
    // timeout or a cancel stops the parse. The crate has no logging, so the
    // dropped error is kept as text on the parsed file and a caller reads it
    // through `PsdDocument::layer_info_error`.
    let mut tail = TaggedTail::default();
    let want_layers = info.layers.is_empty();
    match read_tagged_tail(
        &mut section,
        header,
        options,
        want_layers,
        MAX_PATTERN_BLOCK,
        &mut tail,
    ) {
        Err(error) if error.is_transient() => return Err(error),
        Err(error) => tail.error = Some(error),
        Ok(()) => {}
    }
    let found_layers = tail.layer_info.is_some();
    if let Some(found) = tail.layer_info {
        info = found;
    }
    // A clean layer info block is not a dropped error, even when a later
    // block is bad.
    let layer_info_error = if want_layers && !found_layers {
        tail.error.map(|error| error.to_string())
    } else {
        None
    };
    let mut layers = info.layers;
    drop_unmatched_pattern_fills(&mut layers, &tail.patterns);
    Ok(LayerSectionData {
        layer_count: info.layer_count,
        layers,
        global_mask,
        patterns: tail.patterns,
        merged_has_alpha: info.merged_has_alpha,
        layer_info_error,
        pattern_note: tail.pattern_note,
    })
}

/// Read the global layer mask info: a length then the data.
fn parse_global_mask(section: &mut Window) -> Result<Option<GlobalMask>, PsdError> {
    if section.remaining() < 4 {
        return Ok(None);
    }
    let length = to_usize(u64::from(section.read_u32()?))?;
    section.require(length as u64)?;
    // Only the first 13 bytes carry values. The rest is skipped unread.
    let data = section.peek(length.min(GLOBAL_MASK_BYTES), false)?;
    let mask = (data.len() >= GLOBAL_MASK_BYTES).then(|| GlobalMask {
        overlay_color_space: u16::from_be_bytes([data[0], data[1]]),
        opacity: u16::from_be_bytes([data[10], data[11]]),
        kind: data[12],
    });
    section.skip(length as u64)?;
    Ok(mask)
}

/// Read one layer record.
pub(crate) fn read_layer_record(
    reader: &mut Reader,
    header: &PsdHeader,
    options: &ParseOptions,
) -> Result<LayerRecord, PsdError> {
    let version = header.version;
    let bounds = LayerBounds {
        top: reader.read_i32()?,
        left: reader.read_i32()?,
        bottom: reader.read_i32()?,
        right: reader.read_i32()?,
    };
    validate_layer_bounds(bounds)?;

    let channel_count = usize::from(reader.read_u16()?);
    if channel_count > MAX_LAYER_CHANNELS {
        return Err(PsdError::TooManyChannels {
            count: channel_count,
            limit: MAX_LAYER_CHANNELS,
        });
    }
    let mut channels = Vec::with_capacity(channel_count);
    for _ in 0..channel_count {
        let id = reader.read_i16()?;
        let length = reader.read_length(version)?;
        channels.push(ChannelInfo {
            id,
            length,
            data_offset: 0,
            data_start: 0,
            data_len: 0,
            row_table_offset: 0,
            compression: 0,
            bounds,
        });
    }

    let signature = reader.read_array::<4>()?;
    if &signature != BLEND_SIGNATURE {
        return Err(PsdError::BadBlendSignature { found: signature });
    }
    let blend_key = reader.read_array::<4>()?;
    let opacity = reader.read_u8()?;
    let clipping = if reader.read_u8()? == 0 {
        Clipping::Base
    } else {
        Clipping::NonBase
    };
    let flags = reader.read_u8()?;
    // A filler byte. The extra length follows the filler.
    let _filler = reader.read_u8()?;

    let extra_length = reader.read_u32()?;
    let extra = reader.take(to_usize(u64::from(extra_length))?)?;
    let parsed = read_layer_extra(extra, sample_levels(header.depth), version)?;

    // A mask channel uses the mask rectangle, not the layer rectangle.
    for channel in &mut channels {
        if channel.id == -2
            && let Some(mask) = parsed.mask
        {
            channel.bounds = mask.bounds;
        } else if channel.id == -3
            && let Some(real) = parsed.mask.and_then(|mask| mask.real_bounds)
        {
            channel.bounds = real;
        }
    }

    options.check()?;
    Ok(LayerRecord {
        name: parsed.name,
        bounds,
        channels,
        blend_key,
        opacity,
        fill_opacity: parsed.fill_opacity,
        clipping,
        flags,
        visible: flags & 0x02 == 0,
        mask: parsed.mask,
        blending_ranges: parsed.blending_ranges,
        section: parsed.section,
        content: parsed.content,
        has_vector_mask: parsed.has_vector_mask,
        vector_mask: parsed.vector_mask,
        effects: parsed.effects,
    })
}
