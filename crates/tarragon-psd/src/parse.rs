//! PSD and PSB header and layer parsing.
//!
//! The reader walks the Photoshop binary format by hand. The spike reads the
//! 26-byte header and the first layer record of a file (study task 17). The
//! full parser plan is in the crate `README.md`.
//!
//! Every read is checked. A bad byte offset returns [`PsdError::Truncated`].
//! No input can panic.

use std::path::Path;

use crate::error::PsdError;

/// The four signature bytes of a PSD or PSB file.
const SIGNATURE: &[u8; 4] = b"8BPS";
/// The PSD file version.
const VERSION_PSD: u16 = 1;
/// The PSB file version.
const VERSION_PSB: u16 = 2;
/// The blend mode signature in a layer record.
const BLEND_SIGNATURE: &[u8; 4] = b"8BIM";
/// The additional-layer-info key for the unicode layer name.
const UNICODE_NAME_KEY: &[u8; 4] = b"luni";

/// The PSD or PSB file version.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PsdVersion {
    /// Photoshop document, version 1.
    Psd,
    /// Photoshop large document, version 2.
    Psb,
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

/// One channel of a layer record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChannelInfo {
    /// The channel ID. 0 to 3 are color, -1 is alpha, -2 is a user mask.
    pub id: i16,
    /// The compressed channel data length in bytes.
    pub length: u64,
    /// The file offset where the channel data starts.
    pub data_offset: u64,
}

/// The first layer record of a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LayerRecord {
    /// The layer name. The unicode name wins over the ASCII name.
    pub name: String,
    /// The layer rectangle.
    pub bounds: LayerBounds,
    /// The layer channels, in file order.
    pub channels: Vec<ChannelInfo>,
}

/// The parsed header and the layers of a PSD or PSB file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PsdFile {
    /// The file header.
    pub header: PsdHeader,
    /// The number of layer records.
    pub layer_count: usize,
    /// The first layer record, if the file has layers.
    pub first_layer: Option<LayerRecord>,
}

/// Read the header and the first layer of a PSD or PSB file.
///
/// # Errors
///
/// Returns [`PsdError::Io`] when the file cannot be read. Returns a parse
/// error when the bytes are malformed or truncated.
pub fn read_file(path: &Path) -> Result<PsdFile, PsdError> {
    let data = std::fs::read(path).map_err(|source| PsdError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    parse(&data)
}

/// Read the header and the first layer from a byte slice.
///
/// # Errors
///
/// Returns a parse error when the bytes are malformed or truncated.
pub fn read_bytes(data: &[u8]) -> Result<PsdFile, PsdError> {
    parse(data)
}

/// Walk the file sections up to the first layer record.
fn parse(data: &[u8]) -> Result<PsdFile, PsdError> {
    let mut reader = Reader::new(data);
    let header = read_header(&mut reader)?;

    // Color mode data: a four-byte length then the data.
    let color_mode_length = reader.read_u32()?;
    reader.skip(to_usize(u64::from(color_mode_length))?)?;
    // Image resources: a four-byte length then the data.
    let resources_length = reader.read_u32()?;
    reader.skip(to_usize(u64::from(resources_length))?)?;

    // Layer and mask information: the length field is four bytes in PSD and
    // eight bytes in PSB.
    // The section length is read but does not bound the record walk yet. The
    // full parser adds that bounds check (task 18).
    let section_length = read_length(&mut reader, header.version)?;
    if section_length == 0 {
        return Ok(empty_file(header));
    }
    // Layer info: the same four-byte or eight-byte rule.
    // The layer info length is read but does not bound the record walk yet.
    // The full parser adds that bounds check (task 18).
    let layer_info_length = read_length(&mut reader, header.version)?;
    if layer_info_length == 0 {
        return Ok(empty_file(header));
    }

    // A negative layer count means the merged alpha channel comes first. The
    // absolute value is the record count.
    let layer_count = usize::from(reader.read_i16()?.unsigned_abs());
    if layer_count == 0 {
        return Ok(empty_file(header));
    }

    let mut first_layer = None;
    for index in 0..layer_count {
        let layer = read_layer_record(&mut reader, header.version)?;
        if index == 0 {
            first_layer = Some(layer);
        }
    }

    // The channel image data block follows every layer record. Assign each
    // first-layer channel its file offset inside that block.
    if let Some(layer) = first_layer.as_mut() {
        let mut offset = reader.offset();
        for channel in &mut layer.channels {
            channel.data_offset = offset;
            offset = offset.saturating_add(channel.length);
        }
    }

    Ok(PsdFile {
        header,
        layer_count,
        first_layer,
    })
}

/// Build a file value with no layers.
fn empty_file(header: PsdHeader) -> PsdFile {
    PsdFile {
        header,
        layer_count: 0,
        first_layer: None,
    }
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

/// Read one layer record.
fn read_layer_record(reader: &mut Reader, version: PsdVersion) -> Result<LayerRecord, PsdError> {
    let bounds = LayerBounds {
        top: reader.read_i32()?,
        left: reader.read_i32()?,
        bottom: reader.read_i32()?,
        right: reader.read_i32()?,
    };

    let channel_count = reader.read_u16()?;
    let mut channels = Vec::with_capacity(usize::from(channel_count));
    for _ in 0..channel_count {
        let id = reader.read_i16()?;
        let length = read_length(reader, version)?;
        channels.push(ChannelInfo {
            id,
            length,
            data_offset: 0,
        });
    }

    let signature = reader.read_array::<4>()?;
    if &signature != BLEND_SIGNATURE {
        return Err(PsdError::BadBlendSignature { found: signature });
    }
    // Blend key, opacity, clipping, flags, and filler. The spike does not use
    // them; task 19 reads the blend key.
    let _blend_key = reader.read_array::<4>()?;
    let _opacity = reader.read_u8()?;
    let _clipping = reader.read_u8()?;
    let _flags = reader.read_u8()?;
    let _filler = reader.read_u8()?;

    let extra_length = reader.read_u32()?;
    let extra = reader.take(to_usize(u64::from(extra_length))?)?;
    let name = read_layer_name(extra);

    Ok(LayerRecord {
        name,
        bounds,
        channels,
    })
}

/// Read the layer name from the record extra data.
///
/// The extra data holds the mask data, the blending ranges, a padded ASCII
/// name, and additional info blocks. The unicode `luni` name wins over the
/// ASCII name, as `psd-tools` does. When the mask or blending-ranges section
/// fails, the function yields an empty name. The record bytes were already
/// length-checked by the caller. Task 18 validates the extra data.
fn read_layer_name(extra: &[u8]) -> String {
    let mut reader = Reader::new(extra);
    // Layer mask data: a length then the data.
    if skip_prefixed_section(&mut reader).is_err() {
        return String::new();
    }
    // Layer blending ranges: a length then the data.
    if skip_prefixed_section(&mut reader).is_err() {
        return String::new();
    }
    let ascii = read_pascal_name(&mut reader);
    let unicode = read_unicode_name(&mut reader);
    if unicode.is_empty() { ascii } else { unicode }
}

/// Skip a four-byte length and the data that follows it.
fn skip_prefixed_section(reader: &mut Reader) -> Result<(), PsdError> {
    let length = to_usize(u64::from(reader.read_u32()?))?;
    reader.skip(length)
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

/// Read the unicode name from the additional-layer-info blocks.
fn read_unicode_name(reader: &mut Reader) -> String {
    while reader.remaining() >= 12 {
        let Ok(signature) = reader.read_array::<4>() else {
            break;
        };
        let Ok(key) = reader.read_array::<4>() else {
            break;
        };
        let Ok(length) = reader.read_u32() else {
            break;
        };
        let Ok(length) = to_usize(u64::from(length)) else {
            break;
        };
        let Ok(data) = reader.take(length) else {
            break;
        };
        if &signature == BLEND_SIGNATURE
            && &key == UNICODE_NAME_KEY
            && let Some(name) = decode_utf16be(data)
        {
            return name;
        }
    }
    String::new()
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

/// Convert a declared length to a `usize`.
fn to_usize(value: u64) -> Result<usize, PsdError> {
    usize::try_from(value).map_err(|_| PsdError::LengthTooLarge { length: value })
}

/// A checked big-endian reader over a byte slice.
struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    /// Wrap a byte slice.
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    /// The current offset as a file offset.
    fn offset(&self) -> u64 {
        u64::try_from(self.pos).unwrap_or(u64::MAX)
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

#[cfg(test)]
mod tests {
    use crate::error::PsdError;
    use crate::parse::{ColorMode, PsdVersion, read_bytes};

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

    /// A header plus empty color, resource, and layer sections.
    fn header_only_bytes(version: u16) -> Vec<u8> {
        let mut data = header_bytes(version);
        data.extend_from_slice(&0u32.to_be_bytes());
        data.extend_from_slice(&0u32.to_be_bytes());
        // The layer and mask length is eight bytes in PSB, four in PSD.
        let section_width = if version == 2 { 8 } else { 4 };
        data.extend(std::iter::repeat_n(0u8, section_width));
        data
    }

    #[test]
    fn a_header_only_file_has_no_layers() {
        let file = read_bytes(&header_only_bytes(1)).expect("header parses");
        assert_eq!(file.header.version, PsdVersion::Psd);
        assert_eq!(file.header.color_mode, ColorMode::Rgb);
        assert_eq!((file.header.width, file.header.height), (20, 10));
        assert_eq!(file.layer_count, 0);
        assert!(file.first_layer.is_none());
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
