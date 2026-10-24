//! The extra data of a layer record: mask, blending ranges, name, and blocks.

use crate::adjustments::{parse_adjustment, parse_brightness_contrast};
use crate::content::{LayerContent, parse_gradient_fill, parse_solid_color};
use crate::effects::Effects;
use crate::effects_legacy::parse_legacy_effects;
use crate::effects_parse::parse_object_effects;
use crate::error::PsdError;
use crate::layer::{BlendingRanges, LayerBounds, LayerMask, LayerSection, validate_layer_bounds};
use crate::layer_blocks::read_tagged_block;
use crate::model::PsdVersion;
use crate::reader::{Reader, to_usize};

/// The mask flag bit that says mask parameters follow the flags byte.
const MASK_HAS_PARAMETERS: u8 = 0x10;
/// The additional-layer-info key for the unicode layer name.
const UNICODE_NAME_KEY: &[u8; 4] = b"luni";
/// The additional-layer-info key for the fill opacity.
const FILL_OPACITY_KEY: &[u8; 4] = b"iOpa";
/// The additional-layer-info keys for the layer section divider.
const SECTION_KEYS: [&[u8; 4]; 2] = [b"lsct", b"lsdk"];
/// The additional-layer-info key for the vector mask path.
const VECTOR_MASK_KEYS: [&[u8; 4]; 2] = [b"vmsk", b"vsms"];
/// The object-based layer effects keys. `lfx2` is current and `lfxs` carries
/// the effects of a group layer.
const OBJECT_EFFECT_KEYS: [&[u8; 4]; 2] = [b"lfx2", b"lfxs"];
/// The older object-based layer effects key. It wins over `lfx2` and `lfxs`.
const OLD_OBJECT_EFFECT_KEY: &[u8; 4] = b"lmfx";
/// The legacy layer effects key. Photoshop ignores it on a group layer.
const LEGACY_EFFECT_KEY: &[u8; 4] = b"lrFX";
/// The adjustment-layer block keys this parser recognizes.
const ADJUSTMENT_KEYS: [&[u8; 4]; 16] = [
    b"levl", b"curv", b"hue2", b"brit", b"nvrt", b"thrs", b"post", b"expA", b"vibA", b"blnc",
    b"mixr", b"grdm", b"phfl", b"selc", b"blwh", b"clrL",
];

/// The fields of a layer record extra-data block.
pub(crate) struct LayerExtra {
    /// The layer mask, when present.
    pub(crate) mask: Option<LayerMask>,
    /// The blending ranges.
    pub(crate) blending_ranges: BlendingRanges,
    /// The layer name.
    pub(crate) name: String,
    /// The layer section divider.
    pub(crate) section: Option<LayerSection>,
    /// The fill opacity, 0 to 255.
    pub(crate) fill_opacity: u8,
    /// The parsed layer content.
    pub(crate) content: LayerContent,
    /// True when a vector mask block is present.
    pub(crate) has_vector_mask: bool,
    /// The parsed layer effects.
    pub(crate) effects: Option<Effects>,
}

/// Read the extra data block of a layer record.
pub(crate) fn read_layer_extra(
    extra: &[u8],
    lut_size: u32,
    version: PsdVersion,
) -> Result<LayerExtra, PsdError> {
    let mut reader = Reader::new(extra);
    let mask = read_mask(&mut reader)?;
    let blending_ranges = read_blending_ranges(&mut reader)?;
    let ascii = read_pascal_name(&mut reader);
    let blocks = read_extra_blocks(&mut reader, lut_size, version)?;
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
        effects: blocks.effects,
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
    let bounds = rect_at(data, 0);
    let default_color = data[16];
    let flags = data[17];
    validate_layer_bounds(bounds)?;
    Ok(Some(LayerMask {
        bounds,
        default_color,
        flags,
        real_bounds: read_real_bounds(data, flags),
    }))
}

/// Read the real user mask rectangle, or `None` when it is absent or unusable.
///
/// The compositor does not use the real mask yet, so a bad rectangle must not
/// fail the file.
fn read_real_bounds(data: &[u8], flags: u8) -> Option<LayerBounds> {
    // Flag bit 4 puts the mask parameters after the flags byte: one byte of
    // presence bits, then a density byte or an eight-byte feather per bit.
    let mut at = 18;
    if flags & MASK_HAS_PARAMETERS != 0 {
        let present = *data.get(at)?;
        at += 1;
        for (bit, size) in [(0x01, 1), (0x02, 8), (0x04, 1), (0x08, 8)] {
            if present & bit != 0 {
                at += size;
            }
        }
    }
    // The real flags and the real background byte come before the rectangle.
    let start = at.checked_add(2)?;
    let bytes = data.get(start..start.checked_add(16)?)?;
    let bounds = rect_at(bytes, 0);
    validate_layer_bounds(bounds).ok()?;
    Some(bounds)
}

/// Read four big-endian `i32` edges (top, left, bottom, right) at `at`.
fn rect_at(data: &[u8], at: usize) -> LayerBounds {
    let edge = |index: usize| {
        let start = at + index * 4;
        i32::from_be_bytes([
            data[start],
            data[start + 1],
            data[start + 2],
            data[start + 3],
        ])
    };
    LayerBounds {
        top: edge(0),
        left: edge(1),
        bottom: edge(2),
        right: edge(3),
    }
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
    /// The parsed layer effects.
    effects: Option<Effects>,
}

/// Read the unicode name, section divider, fill opacity, and content blocks.
fn read_extra_blocks(
    reader: &mut Reader,
    lut_size: u32,
    version: PsdVersion,
) -> Result<ExtraBlocks, PsdError> {
    let mut name = String::new();
    let mut section = None;
    let mut fill_opacity = 255;
    let mut content = LayerContent::Pixels;
    let mut has_vector_mask = false;
    let mut brightness_contrast: Option<Vec<u8>> = None;
    let mut cged: Option<Vec<u8>> = None;
    let mut object_effects: Option<Vec<u8>> = None;
    let mut old_object_effects: Option<Vec<u8>> = None;
    let mut legacy_effects: Option<Vec<u8>> = None;
    while reader.remaining() >= 12 {
        let Some(tagged) = read_tagged_block(reader, version)? else {
            break;
        };
        let key = tagged.key;
        let block = tagged.data;
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
        } else if &key == OLD_OBJECT_EFFECT_KEY {
            if old_object_effects.is_none() {
                old_object_effects = Some(block.to_vec());
            }
        } else if OBJECT_EFFECT_KEYS.contains(&&key) {
            if object_effects.is_none() {
                object_effects = Some(block.to_vec());
            }
        } else if &key == LEGACY_EFFECT_KEY {
            if legacy_effects.is_none() {
                legacy_effects = Some(block.to_vec());
            }
        } else if ADJUSTMENT_KEYS.contains(&&key) {
            content = parse_adjustment(&key, block, lut_size)
                .map_or(LayerContent::Unsupported, LayerContent::Adjustment);
        }
    }
    if let Some(block) = brightness_contrast {
        content = parse_brightness_contrast(&block, cged.as_deref())
            .map_or(LayerContent::Unsupported, LayerContent::Adjustment);
    }
    // The oldest object block wins over `lfx2` and `lfxs`. Photoshop ignores
    // the legacy block on a group record, so this parser does the same.
    let is_group = section.is_some_and(|value| value.opens_group() || value.closes_group());
    let mut effects = old_object_effects
        .as_deref()
        .and_then(parse_object_effects)
        .or_else(|| object_effects.as_deref().and_then(parse_object_effects));
    if effects.is_none() && !is_group {
        effects = legacy_effects.as_deref().and_then(parse_legacy_effects);
    }
    Ok(ExtraBlocks {
        name,
        section,
        fill_opacity,
        content,
        has_vector_mask,
        effects,
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
