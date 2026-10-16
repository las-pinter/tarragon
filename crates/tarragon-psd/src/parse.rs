//! PSD and PSB header, layer, and image-data parsing.
//!
//! The reader walks the Photoshop binary format by hand. It reads the full
//! layer-and-mask section, the global layer mask, and the merged image data
//! section (study task 18). The tiled read decodes only a requested viewport.
//!
//! Every read is checked. A bad byte offset returns [`PsdError::Truncated`].
//! Every declared length is validated before an allocation. No input can panic.

use std::path::Path;

use crate::adjustments::{parse_adjustment, parse_brightness_contrast};
use crate::content::{LayerContent, parse_gradient_fill, parse_solid_color};
use crate::decode::{ChannelRegion, ImageRegion, Viewport, decode_channel};
use crate::error::PsdError;
use crate::limits::{MAX_DIMENSION, ParseOptions, full_tier_size};

/// The four signature bytes of a PSD or PSB file.
const SIGNATURE: &[u8; 4] = b"8BPS";
/// The PSD file version.
const VERSION_PSD: u16 = 1;
/// The PSB file version.
const VERSION_PSB: u16 = 2;
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

/// The PSD or PSB file version.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PsdVersion {
    /// Photoshop document, version 1.
    Psd,
    /// Photoshop large document, version 2.
    Psb,
}

impl PsdVersion {
    /// Return the width of a row-length table entry in bytes.
    ///
    /// The channel data lengths use four or eight bytes (the layer record
    /// rule), but the RLE row counts use two or four.
    #[must_use]
    pub const fn rle_entry_bytes(self) -> usize {
        match self {
            Self::Psd => 2,
            Self::Psb => 4,
        }
    }
}

/// The color mode of a PSD or PSB file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorMode {
    /// 1 bit per pixel bitmap.
    Bitmap,
    /// Grayscale.
    Grayscale,
    /// Indexed color.
    Indexed,
    /// RGB color.
    Rgb,
    /// CMYK color.
    Cmyk,
    /// Multichannel.
    Multichannel,
    /// Duotone.
    Duotone,
    /// Lab color.
    Lab,
    /// Any other mode code.
    Other(u16),
}

impl ColorMode {
    /// Map a file color-mode code to a [`ColorMode`].
    #[must_use]
    pub const fn from_code(code: u16) -> Self {
        match code {
            0 => Self::Bitmap,
            1 => Self::Grayscale,
            2 => Self::Indexed,
            3 => Self::Rgb,
            4 => Self::Cmyk,
            7 => Self::Multichannel,
            8 => Self::Duotone,
            9 => Self::Lab,
            other => Self::Other(other),
        }
    }

    /// The number of color channels for the mode, if the mode is composable.
    #[must_use]
    pub const fn color_channels(self) -> Option<usize> {
        match self {
            Self::Rgb => Some(3),
            Self::Cmyk => Some(4),
            _ => None,
        }
    }
}

/// The channel compression code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Compression {
    /// Uncompressed samples.
    Raw,
    /// Apple `PackBits` run-length encoding.
    Rle,
    /// zlib without prediction. The parser does not decode it.
    Zip,
    /// zlib with prediction. The parser does not decode it.
    ZipPrediction,
    /// Any other code.
    Other(u16),
}

impl Compression {
    /// Map a file compression code to a [`Compression`].
    #[must_use]
    pub const fn from_code(code: u16) -> Self {
        match code {
            0 => Self::Raw,
            1 => Self::Rle,
            2 => Self::Zip,
            3 => Self::ZipPrediction,
            other => Self::Other(other),
        }
    }

    /// The numeric code.
    #[must_use]
    pub const fn code(self) -> u16 {
        match self {
            Self::Raw => 0,
            Self::Rle => 1,
            Self::Zip => 2,
            Self::ZipPrediction => 3,
            Self::Other(code) => code,
        }
    }
}

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

/// The 26-byte PSD or PSB file header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PsdHeader {
    /// The four signature bytes, always `8BPS`.
    pub signature: [u8; 4],
    /// The file version.
    pub version: PsdVersion,
    /// The number of channels in the merged image.
    pub channels: u16,
    /// The image height in pixels.
    pub height: u32,
    /// The image width in pixels.
    pub width: u32,
    /// The bit depth per channel.
    pub depth: u16,
    /// The color mode.
    pub color_mode: ColorMode,
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

/// The merged image data section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MergedImage {
    /// The merged image compression.
    pub compression: Compression,
    /// The merged channels, in file order.
    pub channels: Vec<ChannelInfo>,
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

/// The parsed header, layers, global mask, and image data of a PSD or PSB.
#[derive(Debug, Clone, PartialEq)]
pub struct PsdFile {
    /// The file header.
    pub header: PsdHeader,
    /// The declared number of layer records.
    pub layer_count: usize,
    /// The layer records, in file order.
    pub layers: Vec<LayerRecord>,
    /// The global layer mask, when present.
    pub global_mask: Option<GlobalMask>,
    /// The merged image data description.
    pub merged: MergedImage,
}

impl PsdFile {
    /// The first layer record, when the file has layers.
    #[must_use]
    pub fn first_layer(&self) -> Option<&LayerRecord> {
        self.layers.first()
    }

    /// Decode the merged image over `viewport`.
    ///
    /// The returned channels cover exactly the viewport. A pixel outside a
    /// channel rectangle is zero. The compositor (task 19) reads this.
    ///
    /// # Errors
    ///
    /// Returns a typed error when the viewport is invalid, exceeds the cap,
    /// the color mode is unsupported, the deadline passed, or the bytes are
    /// truncated.
    pub fn read_merged_region(
        &self,
        data: &[u8],
        viewport: Viewport,
        options: &ParseOptions,
    ) -> Result<ImageRegion, PsdError> {
        self.check_region(viewport)?;
        let bytes_per_sample = sample_bytes(self.header.color_mode, self.header.depth)?;
        options.check()?;
        self.decode_channels(
            data,
            &self.merged.channels,
            viewport,
            bytes_per_sample,
            options,
        )
    }

    /// Decode one layer's channels over `viewport`.
    ///
    /// # Errors
    ///
    /// Returns a typed error when the layer index is out of range, the
    /// viewport is invalid or exceeds the cap, the color mode is unsupported,
    /// the deadline passed, or the bytes are truncated.
    pub fn read_layer_region(
        &self,
        data: &[u8],
        index: usize,
        viewport: Viewport,
        options: &ParseOptions,
    ) -> Result<ImageRegion, PsdError> {
        let layer = self.layers.get(index).ok_or(PsdError::LayerOutOfRange {
            index,
            count: self.layers.len(),
        })?;
        self.check_region(viewport)?;
        let bytes_per_sample = sample_bytes(self.header.color_mode, self.header.depth)?;
        options.check()?;
        self.decode_channels(data, &layer.channels, viewport, bytes_per_sample, options)
    }

    /// Validate a viewport against the canvas and the full-tier cap.
    pub(crate) fn check_region(&self, viewport: Viewport) -> Result<(), PsdError> {
        // The cap guard runs first so a huge request never reaches an alloc.
        let edge = viewport.width().max(viewport.height());
        if edge > crate::limits::FULL_TIER_MAX_EDGE {
            return Err(PsdError::CapExceeded {
                edge,
                cap: crate::limits::FULL_TIER_MAX_EDGE,
            });
        }
        if viewport.left > viewport.right
            || viewport.top > viewport.bottom
            || viewport.width() == 0
            || viewport.height() == 0
            || viewport.right > self.header.width
            || viewport.bottom > self.header.height
        {
            return Err(PsdError::InvalidViewport {
                left: viewport.left,
                top: viewport.top,
                right: viewport.right,
                bottom: viewport.bottom,
                width: self.header.width,
                height: self.header.height,
            });
        }
        Ok(())
    }

    /// Decode a list of channels over the viewport.
    fn decode_channels(
        &self,
        data: &[u8],
        channels: &[ChannelInfo],
        viewport: Viewport,
        bytes_per_sample: usize,
        options: &ParseOptions,
    ) -> Result<ImageRegion, PsdError> {
        let mut decoded: Vec<ChannelRegion> = Vec::with_capacity(channels.len());
        for channel in channels {
            decoded.push(decode_channel(
                data,
                channel,
                bytes_per_sample,
                self.header.version,
                viewport,
                options,
            )?);
        }
        Ok(ImageRegion {
            viewport,
            depth: self.header.depth,
            color_mode: self.header.color_mode,
            channels: decoded,
        })
    }
}

/// Read the header, layers, and merged image of a PSD or PSB file.
///
/// # Errors
///
/// Returns [`PsdError::Io`] when the file cannot be read. Returns a parse
/// error when the bytes are malformed or truncated.
pub fn read_file(path: &Path) -> Result<PsdFile, PsdError> {
    read_file_with_options(path, &ParseOptions::default())
}

/// Read a file with explicit options.
///
/// # Errors
///
/// Returns [`PsdError::Io`] when the file cannot be read. Returns a parse
/// error when the bytes are malformed or truncated.
pub fn read_file_with_options(path: &Path, options: &ParseOptions) -> Result<PsdFile, PsdError> {
    let data = std::fs::read(path).map_err(|source| PsdError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    parse(&data, options)
}

/// Read the header, layers, and merged image from a byte slice.
///
/// # Errors
///
/// Returns a parse error when the bytes are malformed or truncated.
pub fn read_bytes(data: &[u8]) -> Result<PsdFile, PsdError> {
    parse(data, &ParseOptions::default())
}

/// Read from a byte slice with explicit options.
///
/// # Errors
///
/// Returns a parse error when the bytes are malformed or truncated.
pub fn read_bytes_with_options(data: &[u8], options: &ParseOptions) -> Result<PsdFile, PsdError> {
    parse(data, options)
}

/// Walk every file section.
fn parse(data: &[u8], options: &ParseOptions) -> Result<PsdFile, PsdError> {
    options.check()?;
    let mut reader = Reader::new(data);
    let header = read_header(&mut reader)?;
    validate_dimensions(&header)?;

    // Color mode data: a four-byte length then the data.
    let color_mode_length = reader.read_u32()?;
    reader.skip(to_usize(u64::from(color_mode_length))?)?;
    // Image resources: a four-byte length then the data.
    let resources_length = reader.read_u32()?;
    reader.skip(to_usize(u64::from(resources_length))?)?;

    // Layer and mask information. The section length bounds every layer read.
    // The length field is four bytes in PSD and eight bytes in PSB.
    let section_length = to_usize(read_length(&mut reader, header.version)?)?;
    let (layer_count, layers, global_mask) = if section_length == 0 {
        (0, Vec::new(), None)
    } else {
        let mut section = reader.take_reader(section_length)?;
        parse_layer_section(&mut section, &header, options)?
    };

    // The merged image data section follows the layer section.
    let merged = read_merged_image(&mut reader, &header, options)?;

    Ok(PsdFile {
        header,
        layer_count,
        layers,
        global_mask,
        merged,
    })
}

/// Validate the header dimensions against the Adobe limit.
fn validate_dimensions(header: &PsdHeader) -> Result<(), PsdError> {
    if header.width == 0
        || header.height == 0
        || header.width > MAX_DIMENSION
        || header.height > MAX_DIMENSION
    {
        return Err(PsdError::BadDimensions {
            width: header.width,
            height: header.height,
            limit: MAX_DIMENSION,
        });
    }
    Ok(())
}

/// Validate a layer rectangle against the Adobe limit.
///
/// The row buffer of a tile decode is sized from the layer width, so a huge
/// file-controlled rectangle must be refused before any allocation.
fn validate_layer_bounds(bounds: LayerBounds) -> Result<(), PsdError> {
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

/// Parse the bounded layer-and-mask section.
fn parse_layer_section(
    section: &mut Reader,
    header: &PsdHeader,
    options: &ParseOptions,
) -> Result<(usize, Vec<LayerRecord>, Option<GlobalMask>), PsdError> {
    let layer_info_length = to_usize(read_length(section, header.version)?)?;
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
    if layer_count.saturating_mul(MIN_LAYER_RECORD) > body.data.len() {
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

/// Read the merged image data section.
fn read_merged_image(
    reader: &mut Reader,
    header: &PsdHeader,
    options: &ParseOptions,
) -> Result<MergedImage, PsdError> {
    options.check()?;
    let compression = Compression::from_code(reader.read_u16()?);
    let channels = usize::from(header.channels);
    let channel_ids = merged_channel_ids(header);
    let bounds = canvas_bounds(header)?;
    let mut infos = Vec::new();
    match compression {
        Compression::Raw => {
            let bytes_per_sample = raw_sample_bytes(header)?;
            let row_bytes = usize::try_from(header.width)
                .ok()
                .and_then(|width| width.checked_mul(bytes_per_sample))
                .ok_or(PsdError::LengthTooLarge { length: 0 })?;
            let plane = row_bytes
                .checked_mul(usize::try_from(header.height).unwrap_or(usize::MAX))
                .ok_or(PsdError::LengthTooLarge { length: 0 })?;
            if reader.remaining() < channels.saturating_mul(plane) {
                return Err(PsdError::Truncated {
                    offset: reader.offset(),
                    wanted: channels.saturating_mul(plane),
                    remaining: reader.remaining(),
                });
            }
            let start = reader.offset();
            for (index, id) in channel_ids.iter().enumerate() {
                let offset = start.saturating_add((index as u64).saturating_mul(plane as u64));
                infos.push(merged_channel(
                    *id,
                    offset,
                    plane as u64,
                    compression,
                    bounds,
                ));
            }
        }
        Compression::Rle => {
            let entry = header.version.rle_entry_bytes();
            let rows = usize::try_from(header.height).unwrap_or(usize::MAX);
            let entries = channels
                .checked_mul(rows)
                .ok_or(PsdError::LengthTooLarge { length: 0 })?;
            let table_bytes = entries
                .checked_mul(entry)
                .ok_or(PsdError::LengthTooLarge { length: 0 })?;
            let table_offset = reader.offset();
            let table = reader.take(table_bytes)?;
            let available = reader.remaining();
            let mut data_start = reader.offset();
            let mut total = 0u64;
            for (index, id) in channel_ids.iter().enumerate() {
                let mut length = 0u64;
                for row in 0..rows {
                    let offset = (index * rows + row) * entry;
                    length = length
                        .checked_add(read_row_length(table, offset, entry)?)
                        .ok_or(PsdError::LengthTooLarge { length: 0 })?;
                }
                total = total
                    .checked_add(length)
                    .ok_or(PsdError::LengthTooLarge { length })?;
                let row_table_offset = table_offset
                    .saturating_add(u64::try_from(index * rows * entry).unwrap_or(u64::MAX));
                infos.push(ChannelInfo {
                    id: *id,
                    length,
                    data_offset: data_start,
                    data_start,
                    data_len: length,
                    row_table_offset,
                    compression: compression.code(),
                    bounds,
                });
                data_start = data_start
                    .checked_add(length)
                    .ok_or(PsdError::LengthTooLarge { length })?;
            }
            // The declared channel data must fit after the row table.
            if total > available as u64 {
                return Err(PsdError::Truncated {
                    offset: reader.offset(),
                    wanted: usize::try_from(total).unwrap_or(usize::MAX),
                    remaining: available,
                });
            }
        }
        other => return Err(PsdError::UnsupportedCompression(other.code())),
    }
    Ok(MergedImage {
        compression,
        channels: infos,
    })
}

/// The merged channel IDs for the color mode, with an optional alpha.
fn merged_channel_ids(header: &PsdHeader) -> Vec<i16> {
    let count = usize::from(header.channels);
    let color = header.color_mode.color_channels().unwrap_or(0);
    (0..count)
        .map(|index| {
            if index < color {
                i16::try_from(index).unwrap_or(0)
            } else {
                -1
            }
        })
        .collect()
}

/// Build one merged channel description.
fn merged_channel(
    id: i16,
    offset: u64,
    length: u64,
    compression: Compression,
    bounds: LayerBounds,
) -> ChannelInfo {
    ChannelInfo {
        id,
        length,
        data_offset: offset,
        data_start: offset,
        data_len: length,
        row_table_offset: 0,
        compression: compression.code(),
        bounds,
    }
}

/// The full-canvas rectangle as layer bounds.
fn canvas_bounds(header: &PsdHeader) -> Result<LayerBounds, PsdError> {
    Ok(LayerBounds {
        top: 0,
        left: 0,
        bottom: i32::try_from(header.height).map_err(|_| PsdError::BadDimensions {
            width: header.width,
            height: header.height,
            limit: MAX_DIMENSION,
        })?,
        right: i32::try_from(header.width).map_err(|_| PsdError::BadDimensions {
            width: header.width,
            height: header.height,
            limit: MAX_DIMENSION,
        })?,
    })
}

/// The bytes per sample for the raw merged image.
fn raw_sample_bytes(header: &PsdHeader) -> Result<usize, PsdError> {
    match header.depth {
        8 => Ok(1),
        16 => Ok(2),
        _ => Err(PsdError::UnsupportedColorMode {
            mode: header.color_mode,
            depth: header.depth,
        }),
    }
}

/// Read one RLE row length from the table.
fn read_row_length(table: &[u8], offset: usize, entry: usize) -> Result<u64, PsdError> {
    let bytes = table
        .get(offset..offset + entry)
        .ok_or(PsdError::Truncated {
            offset: offset as u64,
            wanted: entry,
            remaining: table.len().saturating_sub(offset),
        })?;
    Ok(match entry {
        2 => u64::from(u16::from_be_bytes([bytes[0], bytes[1]])),
        4 => u64::from(u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])),
        _ => 0,
    })
}

/// Read the 26-byte file header.
fn read_header(reader: &mut Reader) -> Result<PsdHeader, PsdError> {
    let signature = reader.read_array::<4>()?;
    if &signature != SIGNATURE {
        return Err(PsdError::BadSignature { found: signature });
    }
    let version = match reader.read_u16()? {
        VERSION_PSD => PsdVersion::Psd,
        VERSION_PSB => PsdVersion::Psb,
        other => return Err(PsdError::UnsupportedVersion(other)),
    };
    // Six reserved bytes. The spec wants zero, but do not enforce it.
    reader.skip(6)?;
    let channels = reader.read_u16()?;
    let height = reader.read_u32()?;
    let width = reader.read_u32()?;
    let depth = reader.read_u16()?;
    let color_mode = ColorMode::from_code(reader.read_u16()?);
    Ok(PsdHeader {
        signature,
        version,
        channels,
        height,
        width,
        depth,
        color_mode,
    })
}

/// Read a length field. PSD uses four bytes; PSB uses eight.
fn read_length(reader: &mut Reader, version: PsdVersion) -> Result<u64, PsdError> {
    match version {
        PsdVersion::Psd => Ok(u64::from(reader.read_u32()?)),
        PsdVersion::Psb => reader.read_u64(),
    }
}

/// The sample count of a document depth, used by adjustment lookup tables.
#[must_use]
const fn sample_levels(depth: u16) -> u32 {
    if depth >= 16 { 65_536 } else { 256 }
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
        let length = read_length(reader, version)?;
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

/// The bytes per sample for the supported color modes.
fn sample_bytes(mode: ColorMode, depth: u16) -> Result<usize, PsdError> {
    match (mode, depth) {
        (ColorMode::Rgb | ColorMode::Cmyk, 8) => Ok(1),
        (ColorMode::Rgb, 16) => Ok(2),
        _ => Err(PsdError::UnsupportedColorMode { mode, depth }),
    }
}

/// Convert a declared length to a `usize`.
fn to_usize(value: u64) -> Result<usize, PsdError> {
    usize::try_from(value).map_err(|_| PsdError::LengthTooLarge { length: value })
}

/// A checked big-endian reader over a byte slice.
struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
    /// The file offset of `data[0]`. Nonzero for a bounded section slice.
    base: u64,
}

impl<'a> Reader<'a> {
    /// Wrap a byte slice at file offset zero.
    fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            pos: 0,
            base: 0,
        }
    }

    /// The current offset as a file offset.
    fn offset(&self) -> u64 {
        self.base + u64::try_from(self.pos).unwrap_or(u64::MAX)
    }

    /// The number of bytes left.
    fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.pos)
    }

    /// Take the next `count` bytes, or fail with a typed error.
    fn take(&mut self, count: usize) -> Result<&'a [u8], PsdError> {
        let truncated = PsdError::Truncated {
            offset: self.offset(),
            wanted: count,
            remaining: self.remaining(),
        };
        let end = self.pos.checked_add(count).ok_or(truncated)?;
        if end > self.data.len() {
            return Err(PsdError::Truncated {
                offset: self.offset(),
                wanted: count,
                remaining: self.remaining(),
            });
        }
        let slice = &self.data[self.pos..end];
        self.pos = end;
        Ok(slice)
    }

    /// Take a bounded sub-reader. The child inherits the file offset.
    fn take_reader(&mut self, count: usize) -> Result<Reader<'a>, PsdError> {
        let base = self.offset();
        let slice = self.take(count)?;
        Ok(Reader {
            data: slice,
            pos: 0,
            base,
        })
    }

    /// Skip `count` bytes.
    fn skip(&mut self, count: usize) -> Result<(), PsdError> {
        self.take(count).map(|_| ())
    }

    /// Read a `u8`.
    fn read_u8(&mut self) -> Result<u8, PsdError> {
        Ok(self.take(1)?[0])
    }

    /// Read a big-endian `u16`.
    fn read_u16(&mut self) -> Result<u16, PsdError> {
        let bytes = self.take(2)?;
        Ok(u16::from_be_bytes([bytes[0], bytes[1]]))
    }

    /// Read a big-endian `i16`.
    fn read_i16(&mut self) -> Result<i16, PsdError> {
        let bytes = self.take(2)?;
        Ok(i16::from_be_bytes([bytes[0], bytes[1]]))
    }

    /// Read a big-endian `u32`.
    fn read_u32(&mut self) -> Result<u32, PsdError> {
        let bytes = self.take(4)?;
        Ok(u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    /// Read a big-endian `i32`.
    fn read_i32(&mut self) -> Result<i32, PsdError> {
        let bytes = self.take(4)?;
        Ok(i32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    /// Read a big-endian `u64`.
    fn read_u64(&mut self) -> Result<u64, PsdError> {
        let bytes = self.take(8)?;
        Ok(u64::from_be_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        ]))
    }

    /// Read a fixed-size byte array.
    fn read_array<const N: usize>(&mut self) -> Result<[u8; N], PsdError> {
        let bytes = self.take(N)?;
        let mut out = [0u8; N];
        out.copy_from_slice(bytes);
        Ok(out)
    }
}

impl MergedImage {
    /// The merged channel for `id`, if present.
    #[must_use]
    pub fn channel(&self, id: i16) -> Option<&ChannelInfo> {
        self.channels.iter().find(|channel| channel.id == id)
    }
}

/// The uncapped and capped sizes of a file, for the full tier (Q9).
#[must_use]
pub fn capped_dimensions(header: &PsdHeader) -> (u32, u32) {
    full_tier_size(header.width, header.height)
}

#[cfg(test)]
mod tests {
    use crate::error::PsdError;
    use crate::limits::ParseOptions;
    use crate::parse::{ColorMode, Compression, PsdVersion, read_bytes, read_bytes_with_options};

    /// A minimal valid PSD header with no color data or resources.
    fn header_bytes(version: u16) -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(b"8BPS");
        data.extend_from_slice(&version.to_be_bytes());
        data.extend_from_slice(&[0, 0, 0, 0, 0, 0]);
        data.extend_from_slice(&3u16.to_be_bytes());
        data.extend_from_slice(&10u32.to_be_bytes());
        data.extend_from_slice(&20u32.to_be_bytes());
        data.extend_from_slice(&8u16.to_be_bytes());
        data.extend_from_slice(&3u16.to_be_bytes());
        data
    }

    /// A header plus empty color, resource, and layer sections and a raw
    /// merged image.
    fn header_only_bytes(version: u16) -> Vec<u8> {
        let mut data = header_bytes(version);
        data.extend_from_slice(&0u32.to_be_bytes());
        data.extend_from_slice(&0u32.to_be_bytes());
        // The layer and mask length is eight bytes in PSB, four in PSD.
        let section_width = if version == 2 { 8 } else { 4 };
        data.extend(std::iter::repeat_n(0u8, section_width));
        // A raw merged image: compression 0 then 3 channels.
        data.extend_from_slice(&0u16.to_be_bytes());
        data.extend(std::iter::repeat_n(7u8, 20 * 10 * 3));
        data
    }

    #[test]
    fn a_header_only_file_has_no_layers() {
        let file = read_bytes(&header_only_bytes(1)).expect("header parses");
        assert_eq!(file.header.version, PsdVersion::Psd);
        assert_eq!(file.header.color_mode, ColorMode::Rgb);
        assert_eq!((file.header.width, file.header.height), (20, 10));
        assert_eq!(file.layer_count, 0);
        assert!(file.first_layer().is_none());
        assert_eq!(file.merged.compression, Compression::Raw);
        assert_eq!(file.merged.channels.len(), 3);
    }

    #[test]
    fn the_global_mask_parses_when_present() {
        let mut data = header_bytes(1);
        data.extend_from_slice(&0u32.to_be_bytes());
        data.extend_from_slice(&0u32.to_be_bytes());
        data.extend_from_slice(&21u32.to_be_bytes());
        data.extend_from_slice(&0u32.to_be_bytes());
        data.extend_from_slice(&13u32.to_be_bytes());
        let mut mask = vec![0u8; 13];
        mask[10] = 0;
        mask[11] = 100;
        mask[12] = 128;
        data.extend_from_slice(&mask);
        data.extend_from_slice(&0u16.to_be_bytes());
        data.extend(std::iter::repeat_n(7u8, 20 * 10 * 3));
        let file = read_bytes(&data).expect("parse the global mask file");
        let mask = file.global_mask.expect("a global mask");
        assert_eq!(mask.overlay_color_space, 0);
        assert_eq!(mask.opacity, 100);
        assert_eq!(mask.kind, 128);
    }

    #[test]
    fn a_bad_signature_is_a_typed_error() {
        let mut data = header_bytes(1);
        data[..4].copy_from_slice(b"NOPE");
        let error = read_bytes(&data).expect_err("signature is wrong");
        assert!(matches!(error, PsdError::BadSignature { found } if &found == b"NOPE"));
    }

    #[test]
    fn an_unknown_version_is_a_typed_error() {
        let data = header_bytes(7);
        let error = read_bytes(&data).expect_err("version is wrong");
        assert!(matches!(error, PsdError::UnsupportedVersion(7)));
    }

    #[test]
    fn an_empty_slice_is_a_typed_error() {
        let error = read_bytes(b"").expect_err("no header");
        assert!(matches!(error, PsdError::Truncated { .. }));
    }

    #[test]
    fn an_expired_deadline_is_a_typed_timeout() {
        let options = ParseOptions::with_deadline(crate::limits::Deadline::after(
            std::time::Duration::from_secs(0),
        ));
        let error = read_bytes_with_options(&header_only_bytes(1), &options)
            .expect_err("deadline is expired");
        assert!(matches!(error, PsdError::Timeout));
    }

    #[test]
    fn truncations_never_panic() {
        let full = header_only_bytes(2);
        // A prefix shorter than the 26-byte header always fails with a type.
        for length in 0..26 {
            assert!(read_bytes(&full[..length]).is_err(), "length {length}");
        }
        // The rest of the prefixes must return, never panic.
        for length in 26..=full.len() {
            let _ = read_bytes(&full[..length]);
        }
        // The complete file is valid.
        assert!(read_bytes(&full).is_ok());
    }
}
