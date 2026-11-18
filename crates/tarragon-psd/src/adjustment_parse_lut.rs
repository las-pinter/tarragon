//! Parser for the `clrL` (Color Lookup) block.
//!
//! The block is a `u16` version 1, then a versioned descriptor. Only a 3D
//! table that embeds a `.cube` file is read. A profile-based lookup, a `.3dl`
//! or `.look` table, or a bad file gives `None`, and the layer stays
//! unsupported. The dither flag, the profile bytes and the `dataOrder` and
//! `tableOrder` enums are not applied: a `.cube` file always lists red
//! fastest, which is also what a Photoshop-written file with `bgrOrder` holds.

use std::sync::Arc;

use crate::adjustment_parse::be16;
use crate::adjustments::Adjustment;
use crate::descriptor::{Value, parse_versioned};
use crate::lut3d::Lut3d;

/// Parse a `clrL` block.
#[must_use]
pub(crate) fn parse_color_lookup(data: &[u8]) -> Option<Adjustment> {
    if be16(data, 0)? != 1 {
        return None;
    }
    let (descriptor, _) = parse_versioned(data.get(2..)?)?;
    if let Some(kind) = descriptor.enumeration(b"lookupType")
        && kind != b"3DLUT"
    {
        return None;
    }
    if let Some(format) = descriptor.enumeration(b"LUTFormat")
        && format != b"LUTFormatCUBE"
    {
        return None;
    }
    let Some(Value::Raw(bytes)) = descriptor.get(b"LUT3DFileData") else {
        return None;
    };
    let lut = Lut3d::parse_cube(bytes)?;
    Some(Adjustment::ColorLookup { lut: Arc::new(lut) })
}
