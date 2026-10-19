//! Layer records and the layer-and-mask section.

use crate::adjustments::{parse_adjustment, parse_brightness_contrast};
use crate::content::{LayerContent, parse_gradient_fill, parse_solid_color};
use crate::error::PsdError;
use crate::limits::{MAX_DIMENSION, ParseOptions};
use crate::model::{Compression, PsdHeader, PsdVersion, sample_levels};
use crate::reader::{Reader, to_usize};

/// The blend mode signature in a layer record.
const BLEND_SIGNATURE: &[u8; 4] = b"8BIM";
/// The large-document tag signature.
const BLEND_SIGNATURE_64: &[u8; 4] = b"8B64";
/// The additional-layer-info key for the unicode layer name.
const UNICODE_NAME_KEY: &[u8; 4] = b"luni";
/// The additional-layer-info key for the fill opacity.
const FILL_OPACITY_KEY: &[u8; 4] = b"iOpa";
/// The additional-layer-info keys for the layer section divider.
const SECTION_KEYS: [&[u8; 4]; 2] = [b"lsct", b"lsdk"];
/// The additional-layer-info key for the vector mask path.
const VECTOR_MASK_KEYS: [&[u8; 4]; 2] = [b"vmsk", b"vsms"];
/// The adjustment-layer block keys this parser recognizes.
const ADJUSTMENT_KEYS: [&[u8; 4]; 16] = [
    b"levl", b"curv", b"hue2", b"brit", b"nvrt", b"thrs", b"post", b"expA", b"vibA", b"blnc",
    b"mixr", b"grdm", b"phfl", b"selc", b"blwh", b"clrL",
];
/// The smallest possible layer record in bytes.
const MIN_LAYER_RECORD: usize = 34;

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
    /// The width in pixels, never negative.
    #[must_use]
    pub const fn width(self) -> i64 {
        self.right as i64 - self.left as i64
    }

    /// The height in pixels, never negative.
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
}

/// Parse the bounded layer-and-mask section.
pub(crate) fn parse_layer_section(
    section: &mut Reader,
    header: &PsdHeader,
    options: &ParseOptions,
) -> Result<(usize, Vec<LayerRecord>, Option<GlobalMask>), PsdError> {
    let layer_info_length = to_usize(section.read_length(header.version)?)?;
    let (layer_count, layers) = if layer_info_length == 0 {
        (0, Vec::new())
    } else {
        // The layer info length bounds the record and channel walk.
        let mut body = section.take_reader(layer_info_length)?;
        parse_layer_info(&mut body, header, options)?
    };
    let global_mask = parse_global_mask(section)?;
    Ok((layer_count, layers, global_mask))
}

/// Parse the bounded layer info body: records then channel image data.
fn parse_layer_info(
    body: &mut Reader,
    header: &PsdHeader,
    options: &ParseOptions,
) -> Result<(usize, Vec<LayerRecord>), PsdError> {
    // A negative count marks merged transparency. The absolute value is the
    // record count.
    let declared = body.read_i16()?;
    let layer_count = usize::from(declared.unsigned_abs());
    // A record needs at least 34 bytes. Reject an impossible count early.
    if layer_count.saturating_mul(MIN_LAYER_RECORD) > body.data_len() {
        return Err(PsdError::Truncated {
            offset: body.offset(),
            wanted: layer_count.saturating_mul(MIN_LAYER_RECORD),
            remaining: body.remaining(),
        });
    }

    let mut layers = Vec::new();
    for _ in 0..layer_count {
        options.check()?;
        layers.push(read_layer_record(body, header, options)?);
    }
    assign_channel_offsets(&mut layers, body, header.version)?;
    Ok((layer_count, layers))
}

/// Assign each layer channel its file offset inside the channel data block.
fn assign_channel_offsets(
    layers: &mut [LayerRecord],
    reader: &mut Reader,
    version: PsdVersion,
) -> Result<(), PsdError> {
    for layer in layers.iter_mut() {
        for channel in &mut layer.channels {
            let offset = reader.offset();
            channel.data_offset = offset;
            // A block shorter than the two-byte compression field is empty.
            // Do not decode it as raw, or it would read the next channel.
            if channel.length < 2 {
                if channel.length == 1 {
                    reader.skip(1)?;
                }
                channel.compression = Compression::Raw.code();
                channel.data_start = offset;
                channel.data_len = 0;
                channel.row_table_offset = 0;
                continue;
            }
            let compression = reader.read_u16()?;
            channel.compression = compression;
            let rows = u64::try_from(channel.bounds.height().max(0)).unwrap_or(0);
            if compression == Compression::Rle.code() {
                let table = rows.saturating_mul(version.rle_entry_bytes() as u64);
                // The compression field and the row table must fit the block.
                if 2 + table > channel.length {
                    return Err(PsdError::Truncated {
                        offset,
                        wanted: usize::try_from(2 + table).unwrap_or(usize::MAX),
                        remaining: usize::try_from(channel.length).unwrap_or(usize::MAX),
                    });
                }
                channel.row_table_offset = offset.saturating_add(2);
                channel.data_start = offset.saturating_add(2).saturating_add(table);
                channel.data_len = channel.length - 2 - table;
            } else {
                channel.row_table_offset = 0;
                channel.data_start = offset.saturating_add(2);
                channel.data_len = channel.length - 2;
            }
            reader.skip(to_usize(channel.length - 2)?)?;
        }
    }
    Ok(())
}

/// Read the global layer mask info: a length then the data.
fn parse_global_mask(reader: &mut Reader) -> Result<Option<GlobalMask>, PsdError> {
    if reader.remaining() < 4 {
        return Ok(None);
    }
    let length = to_usize(u64::from(reader.read_u32()?))?;
    let data = reader.take(length)?;
    if data.len() < 13 {
        return Ok(None);
    }
    Ok(Some(GlobalMask {
        overlay_color_space: u16::from_be_bytes([data[0], data[1]]),
        opacity: u16::from_be_bytes([data[10], data[11]]),
        kind: data[12],
    }))
}

/// Read one layer record.
fn read_layer_record(
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

    let channel_count = reader.read_u16()?;
    let mut channels = Vec::new();
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
    let parsed = read_layer_extra(extra, sample_levels(header.depth))?;

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
    })
}

/// The fields of a layer record extra-data block.
struct LayerExtra {
    /// The layer mask, when present.
    mask: Option<LayerMask>,
    /// The blending ranges.
    blending_ranges: BlendingRanges,
    /// The layer name.
    name: String,
    /// The layer section divider.
    section: Option<LayerSection>,
    /// The fill opacity, 0 to 255.
    fill_opacity: u8,
    /// The parsed layer content.
    content: LayerContent,
    /// True when a vector mask block is present.
    has_vector_mask: bool,
}

/// Read the extra data block of a layer record.
fn read_layer_extra(extra: &[u8], lut_size: u32) -> Result<LayerExtra, PsdError> {
    let mut reader = Reader::new(extra);
    let mask = read_mask(&mut reader)?;
    let blending_ranges = read_blending_ranges(&mut reader)?;
    let ascii = read_pascal_name(&mut reader);
    let blocks = read_extra_blocks(&mut reader, lut_size)?;
    Ok(LayerExtra {
        mask,
        blending_ranges,
        name: if blocks.name.is_empty() {
            ascii
        } else {
            blocks.name
        },
        section: blocks.section,
        fill_opacity: blocks.fill_opacity,
        content: blocks.content,
        has_vector_mask: blocks.has_vector_mask,
    })
}

/// Read the layer blending ranges: a length then the ranges.
fn read_blending_ranges(reader: &mut Reader) -> Result<BlendingRanges, PsdError> {
    let length = to_usize(u64::from(reader.read_u32()?))?;
    let data = reader.take(length)?;
    let mut ranges = Reader::new(data);
    let mut composite = [0u16; 4];
    if data.len() >= 8 {
        for slot in &mut composite {
            *slot = ranges.read_u16()?;
        }
    }
    let mut channels = Vec::new();
    while ranges.remaining() >= 8 {
        let mut range = [0u16; 4];
        for slot in &mut range {
            *slot = ranges.read_u16()?;
        }
        channels.push(range);
    }
    Ok(BlendingRanges {
        composite,
        channels,
    })
}

/// Read the layer mask data: a length then the mask body.
fn read_mask(reader: &mut Reader) -> Result<Option<LayerMask>, PsdError> {
    let length = to_usize(u64::from(reader.read_u32()?))?;
    let data = reader.take(length)?;
    if data.len() < 18 {
        return Ok(None);
    }
    let bounds = LayerBounds {
        top: i32::from_be_bytes([data[0], data[1], data[2], data[3]]),
        left: i32::from_be_bytes([data[4], data[5], data[6], data[7]]),
        bottom: i32::from_be_bytes([data[8], data[9], data[10], data[11]]),
        right: i32::from_be_bytes([data[12], data[13], data[14], data[15]]),
    };
    let default_color = data[16];
    let flags = data[17];
    let real_bounds = if data.len() >= 36 {
        Some(LayerBounds {
            top: i32::from_be_bytes([data[20], data[21], data[22], data[23]]),
            left: i32::from_be_bytes([data[24], data[25], data[26], data[27]]),
            bottom: i32::from_be_bytes([data[28], data[29], data[30], data[31]]),
            right: i32::from_be_bytes([data[32], data[33], data[34], data[35]]),
        })
    } else {
        None
    };
    validate_layer_bounds(bounds)?;
    if let Some(real) = real_bounds {
        validate_layer_bounds(real)?;
    }
    Ok(Some(LayerMask {
        bounds,
        default_color,
        flags,
        real_bounds,
    }))
}

/// The unicode name, section divider, fill opacity, and content from the extra
/// blocks.
struct ExtraBlocks {
    /// The unicode layer name.
    name: String,
    /// The section divider, when present.
    section: Option<LayerSection>,
    /// The fill opacity, 0 to 255. Defaults to 255 when absent.
    fill_opacity: u8,
    /// The parsed layer content.
    content: LayerContent,
    /// True when a vector mask block is present.
    has_vector_mask: bool,
}

/// Read the unicode name, section divider, fill opacity, and content blocks.
fn read_extra_blocks(reader: &mut Reader, lut_size: u32) -> Result<ExtraBlocks, PsdError> {
    let mut name = String::new();
    let mut section = None;
    let mut fill_opacity = 255;
    let mut content = LayerContent::Pixels;
    let mut has_vector_mask = false;
    let mut brightness_contrast: Option<Vec<u8>> = None;
    let mut cged: Option<Vec<u8>> = None;
    while reader.remaining() >= 12 {
        let signature = reader.read_array::<4>()?;
        if &signature != BLEND_SIGNATURE && &signature != BLEND_SIGNATURE_64 {
            break;
        }
        let key = reader.read_array::<4>()?;
        let length = to_usize(u64::from(reader.read_u32()?))?;
        let block = reader.take(length)?;
        if &key == UNICODE_NAME_KEY {
            if let Some(decoded) = decode_utf16be(block) {
                name = decoded;
            }
        } else if &key == FILL_OPACITY_KEY && !block.is_empty() {
            fill_opacity = block[0];
        } else if SECTION_KEYS.contains(&&key) && block.len() >= 4 {
            section = Some(LayerSection::from_code(u32::from_be_bytes([
                block[0], block[1], block[2], block[3],
            ])));
        } else if VECTOR_MASK_KEYS.contains(&&key) {
            has_vector_mask = true;
        } else if &key == b"SoCo" {
            content = parse_solid_color(block)
                .map_or(LayerContent::Unsupported, LayerContent::SolidColor);
        } else if &key == b"GdFl" {
            content = parse_gradient_fill(block)
                .map_or(LayerContent::Unsupported, LayerContent::Gradient);
        } else if &key == b"PtFl" {
            // A pattern fill needs the document pattern table (a documented gap).
            content = LayerContent::Unsupported;
        } else if &key == b"brit" {
            brightness_contrast = Some(block.to_vec());
        } else if &key == b"CgEd" {
            cged = Some(block.to_vec());
        } else if ADJUSTMENT_KEYS.contains(&&key) {
            content = parse_adjustment(&key, block, lut_size)
                .map_or(LayerContent::Unsupported, LayerContent::Adjustment);
        }
    }
    if let Some(block) = brightness_contrast {
        content = parse_brightness_contrast(&block, cged.as_deref())
            .map_or(LayerContent::Unsupported, LayerContent::Adjustment);
    }
    Ok(ExtraBlocks {
        name,
        section,
        fill_opacity,
        content,
        has_vector_mask,
    })
}

/// Read the padded Pascal string layer name.
fn read_pascal_name(reader: &mut Reader) -> String {
    let Ok(length) = reader.read_u8() else {
        return String::new();
    };
    let length = usize::from(length);
    let Ok(bytes) = reader.take(length) else {
        return String::new();
    };
    let name = String::from_utf8_lossy(bytes).into_owned();
    // The length byte plus the string round up to a four-byte boundary.
    let consumed = length + 1;
    let pad = (4 - (consumed % 4)) % 4;
    let _ = reader.skip(pad);
    name
}

/// Decode a `luni` block: a four-byte count then UTF-16BE units.
fn decode_utf16be(data: &[u8]) -> Option<String> {
    let count = u32::from_be_bytes(data.get(..4)?.try_into().ok()?);
    let count = usize::try_from(count).ok()?;
    let byte_count = count.checked_mul(2)?;
    let units = data.get(4..4 + byte_count)?;
    let units: Vec<u16> = units
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| u16::from_be_bytes([pair[0], pair[1]]))
        .collect();
    Some(String::from_utf16_lossy(&units))
}
