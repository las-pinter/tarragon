//! PSD and PSB header, layer, and image-data parsing.
//!
//! The reader walks the Photoshop binary format by hand. It reads the full
//! layer-and-mask section, the global layer mask, and the merged image data
//! section. The tiled read decodes only a requested viewport.
//!
//! Every read is checked. A bad byte offset returns [`PsdError::Truncated`].
//! Every declared length is validated before an allocation. Malformed input
//! returns a typed error.

use std::path::Path;

use crate::error::PsdError;
use crate::layer::{LayerSectionData, parse_layer_section};
use crate::limits::{MAX_DIMENSION, MAX_HEADER_CHANNELS, ParseOptions, full_tier_size};
use crate::merged::read_merged_image;
use crate::reader::{Reader, to_usize};
use crate::resources::parse_global_light;

pub use crate::layer::{
    BlendingRanges, ChannelInfo, Clipping, GlobalMask, LayerBounds, LayerMask, LayerRecord,
    LayerSection,
};
pub use crate::merged::MergedImage;
pub use crate::model::{ColorMode, Compression, PsdFile, PsdHeader, PsdVersion};

/// The four signature bytes of a PSD or PSB file.
const SIGNATURE: &[u8; 4] = b"8BPS";
/// The PSD file version.
const VERSION_PSD: u16 = 1;
/// The PSB file version.
const VERSION_PSB: u16 = 2;

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
    let resources_length = to_usize(u64::from(reader.read_u32()?))?;
    let resources = reader.take(resources_length)?;
    let global_light = parse_global_light(resources);

    // Layer and mask information. The section length bounds every layer read.
    // The length field is four bytes in PSD and eight bytes in PSB.
    let section_length = to_usize(reader.read_length(header.version)?)?;
    let section = if section_length == 0 {
        LayerSectionData {
            layer_count: 0,
            layers: Vec::new(),
            global_mask: None,
            merged_has_alpha: false,
            layer_info_error: None,
        }
    } else {
        let mut section = reader.take_reader(section_length)?;
        parse_layer_section(&mut section, &header, options)?
    };

    // The merged image data section follows the layer section.
    let merged = read_merged_image(&mut reader, &header, section.merged_has_alpha, options)?;

    Ok(PsdFile {
        header,
        layer_count: section.layer_count,
        layers: section.layers,
        global_mask: section.global_mask,
        global_light,
        merged_has_alpha: section.merged_has_alpha,
        layer_info_error: section.layer_info_error,
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
    if usize::from(channels) > MAX_HEADER_CHANNELS {
        return Err(PsdError::TooManyChannels {
            count: usize::from(channels),
            limit: MAX_HEADER_CHANNELS,
        });
    }
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

/// The uncapped and capped sizes of a file, for the full tier.
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
