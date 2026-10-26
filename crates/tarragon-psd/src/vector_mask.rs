//! The vector mask path of a layer: the `vmsk` and `vsms` blocks.
//!
//! The block holds a version, a flag word, and a list of 26-byte path
//! records. A subpath is one length record followed by its knot records. Each
//! knot has three points, a preceding control, an anchor, and a leaving
//! control. Each point is a pair of signed 8.24 fixed-point numbers, stored
//! as the vertical then the horizontal fraction of the document size.
//!
//! Parsing is strict about the records and quiet about failure. A block with a
//! wrong version, a truncated subpath, or a knot of the wrong kind gives
//! `None`, so the layer has no vector mask.

use crate::layer::LayerRecord;

/// The only block version the parser reads.
const BLOCK_VERSION: u32 = 3;
/// The size of one path record in bytes: a selector and 24 data bytes.
const RECORD_SIZE: usize = 26;
/// Block padding is shorter than this many bytes.
const MAX_PADDING: usize = 4;
/// The size of the version and flags header in bytes.
const HEADER_SIZE: usize = 8;
/// The flag bit that inverts the mask.
const FLAG_INVERT: u32 = 0x1;
/// The flag bit that says the mask is not linked to the layer.
const FLAG_NOT_LINKED: u32 = 0x2;
/// The flag bit that disables the mask.
const FLAG_DISABLED: u32 = 0x4;
/// The record selector of a closed subpath length.
const CLOSED_LENGTH: u16 = 0;
/// The record selectors of a closed knot, linked and unlinked.
const CLOSED_KNOTS: [u16; 2] = [1, 2];
/// The record selector of an open subpath length.
const OPEN_LENGTH: u16 = 3;
/// The record selectors of an open knot, linked and unlinked.
const OPEN_KNOTS: [u16; 2] = [4, 5];
/// The record selector of the initial fill.
const INITIAL_FILL: u16 = 8;
/// The scale of a signed 8.24 fixed-point number.
const FIXED_ONE: f64 = 16_777_216.0;

/// How a subpath combines with the paths before it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathOp {
    /// Add the area to the shape.
    Combine,
    /// Remove the area from the shape.
    Subtract,
    /// Keep only the area the shape already has.
    Intersect,
    /// Keep the area that only one of the two covers.
    Exclude,
}

impl PathOp {
    /// Decode the signed operation code. An unknown code combines.
    fn from_code(code: i16) -> Self {
        match code {
            2 => Self::Subtract,
            3 => Self::Intersect,
            0 => Self::Exclude,
            _ => Self::Combine,
        }
    }
}

/// One Bezier knot. Each point is `(x, y)` as a fraction of the document
/// width and height.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Knot {
    /// The control point before the anchor.
    pub preceding: (f64, f64),
    /// The anchor point.
    pub anchor: (f64, f64),
    /// The control point after the anchor.
    pub leaving: (f64, f64),
}

/// One subpath: a run of knots.
#[derive(Debug, Clone, PartialEq)]
pub struct Subpath {
    /// The knots in path order.
    pub knots: Vec<Knot>,
    /// True for a closed subpath. An open subpath is closed to fill it.
    pub closed: bool,
    /// How the subpath combines with the paths before it.
    pub op: PathOp,
}

/// The parsed vector mask of a layer.
// The four flags mirror the format: three flag bits and the initial fill.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, PartialEq)]
pub struct VectorMask {
    /// The subpaths in file order.
    pub subpaths: Vec<Subpath>,
    /// True when the mask is inverted.
    pub invert: bool,
    /// True when the mask is not linked to the layer.
    pub not_linked: bool,
    /// True when the mask is disabled.
    pub disabled: bool,
    /// True when the empty start covers the whole document.
    pub initial_fill: bool,
}

impl LayerRecord {
    /// True when the layer has a raster mask or a vector mask that applies.
    #[must_use]
    pub(crate) fn has_mask(&self) -> bool {
        self.mask.is_some() || self.vector_mask.as_ref().is_some_and(|v| !v.disabled)
    }
}

/// Parse a `vmsk` or `vsms` block, or `None` when the header is bad or no
/// subpath survives a fault.
#[must_use]
pub(crate) fn parse_vector_mask(block: &[u8]) -> Option<VectorMask> {
    if u32_at(block, 0)? != BLOCK_VERSION {
        return None;
    }
    let flags = u32_at(block, 4)?;
    let mut mask = VectorMask {
        subpaths: Vec::new(),
        invert: flags & FLAG_INVERT != 0,
        not_linked: flags & FLAG_NOT_LINKED != 0,
        disabled: flags & FLAG_DISABLED != 0,
        initial_fill: false,
    };
    let (records, tail) = block.get(HEADER_SIZE..)?.as_chunks::<RECORD_SIZE>();
    // Up to three bytes after the last record are block padding. More is a
    // cut-off record: the subpaths before it stay.
    let mut faulty = tail.len() >= MAX_PADDING;
    let mut records = records.iter();
    while let Some(record) = records.next() {
        match u16_at(record, 0)? {
            selector @ (CLOSED_LENGTH | OPEN_LENGTH) => {
                let closed = selector == CLOSED_LENGTH;
                let count = usize::from(u16_at(record, 2)?);
                let op = PathOp::from_code(i16::from_be_bytes([record[4], record[5]]));
                // A bad subpath ends the parse. Earlier subpaths stay.
                // If a dropped later subpath was a subtract or intersect, the
                // mask can cover more than intended.
                let Some(knots) = read_knots(&mut records, count, closed) else {
                    faulty = true;
                    break;
                };
                mask.subpaths.push(Subpath { knots, closed, op });
            }
            INITIAL_FILL => mask.initial_fill = u16_at(record, 2)? != 0,
            // Fill rule, clipboard, and stray knot records carry nothing used.
            _ => {}
        }
    }
    // A fault with nothing recovered is a bad block: no vector mask.
    if faulty && mask.subpaths.is_empty() {
        return None;
    }
    Some(mask)
}

/// Read the `count` knot records of one subpath, or `None` when one is short
/// or of the wrong kind.
fn read_knots(
    records: &mut std::slice::Iter<'_, [u8; RECORD_SIZE]>,
    count: usize,
    closed: bool,
) -> Option<Vec<Knot>> {
    let mut knots = Vec::with_capacity(count.min(records.len()));
    for _ in 0..count {
        knots.push(parse_knot(records.next()?, closed)?);
    }
    Some(knots)
}

/// Parse one knot record. The selector must match the subpath kind.
fn parse_knot(record: &[u8], closed: bool) -> Option<Knot> {
    let allowed = if closed { CLOSED_KNOTS } else { OPEN_KNOTS };
    if !allowed.contains(&u16_at(record, 0)?) {
        return None;
    }
    let point = |index: usize| -> Option<(f64, f64)> {
        let at = 2 + index * 8;
        let y = fixed_at(record, at)?;
        let x = fixed_at(record, at + 4)?;
        Some((x, y))
    };
    Some(Knot {
        preceding: point(0)?,
        anchor: point(1)?,
        leaving: point(2)?,
    })
}

/// A big-endian `u16` at `at`.
fn u16_at(data: &[u8], at: usize) -> Option<u16> {
    let bytes = data.get(at..at.checked_add(2)?)?;
    Some(u16::from_be_bytes([bytes[0], bytes[1]]))
}

/// A big-endian `u32` at `at`.
fn u32_at(data: &[u8], at: usize) -> Option<u32> {
    let bytes = data.get(at..at.checked_add(4)?)?;
    Some(u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

/// A signed 8.24 fixed-point number at `at`.
fn fixed_at(data: &[u8], at: usize) -> Option<f64> {
    let raw = i32::from_be_bytes(u32_at(data, at)?.to_be_bytes());
    Some(f64::from(raw) / FIXED_ONE)
}

#[cfg(test)]
#[allow(clippy::cast_possible_truncation)]
mod tests {
    use super::*;

    /// A fixed-point value from a fraction.
    fn fixed(value: f64) -> [u8; 4] {
        ((value * FIXED_ONE) as i32).to_be_bytes()
    }

    /// A knot record with the three points all at `(x, y)`.
    fn knot_record(selector: u16, x: f64, y: f64) -> Vec<u8> {
        let mut record = selector.to_be_bytes().to_vec();
        for _ in 0..3 {
            record.extend_from_slice(&fixed(y));
            record.extend_from_slice(&fixed(x));
        }
        record
    }

    /// A length record.
    fn length_record(selector: u16, count: u16, op: i16) -> Vec<u8> {
        let mut record = selector.to_be_bytes().to_vec();
        record.extend_from_slice(&count.to_be_bytes());
        record.extend_from_slice(&op.to_be_bytes());
        record.resize(RECORD_SIZE, 0);
        record
    }

    /// A block header with the given flags.
    fn header(flags: u32) -> Vec<u8> {
        let mut data = BLOCK_VERSION.to_be_bytes().to_vec();
        data.extend_from_slice(&flags.to_be_bytes());
        data
    }

    #[test]
    fn fixed_point_decodes_signs_and_fractions() {
        assert_eq!(fixed_at(&fixed(0.5), 0), Some(0.5));
        assert_eq!(fixed_at(&fixed(-0.25), 0), Some(-0.25));
        assert_eq!(fixed_at(&fixed(1.0), 0), Some(1.0));
        assert_eq!(fixed_at(&[0, 0], 0), None);
    }

    #[test]
    fn a_closed_subpath_parses_with_swapped_axes() {
        let mut data = header(FLAG_INVERT | FLAG_DISABLED);
        data.extend(length_record(0, 1, -1));
        data.extend(knot_record(1, 0.25, 0.75));
        let mask = parse_vector_mask(&data).expect("parses");
        assert!(mask.invert && mask.disabled && !mask.not_linked);
        assert_eq!(mask.subpaths.len(), 1);
        assert_eq!(mask.subpaths[0].op, PathOp::Combine);
        assert!(mask.subpaths[0].closed);
        assert_eq!(mask.subpaths[0].knots[0].anchor, (0.25, 0.75));
    }

    #[test]
    fn the_operation_codes_decode() {
        for (code, want) in [
            (1, PathOp::Combine),
            (-1, PathOp::Combine),
            (2, PathOp::Subtract),
            (3, PathOp::Intersect),
            (0, PathOp::Exclude),
        ] {
            assert_eq!(PathOp::from_code(code), want);
        }
    }

    #[test]
    fn a_bad_block_gives_none() {
        // Wrong version.
        let mut data = 2u32.to_be_bytes().to_vec();
        data.extend_from_slice(&0u32.to_be_bytes());
        assert!(parse_vector_mask(&data).is_none());
        // A subpath that promises two knots and has one.
        let mut data = header(0);
        data.extend(length_record(0, 2, 1));
        data.extend(knot_record(1, 0.0, 0.0));
        assert!(parse_vector_mask(&data).is_none());
        // An open knot inside a closed subpath.
        let mut data = header(0);
        data.extend(length_record(0, 1, 1));
        data.extend(knot_record(4, 0.0, 0.0));
        assert!(parse_vector_mask(&data).is_none());
        // Cut short at every length: never a panic.
        let mut full = header(0);
        full.extend(length_record(3, 1, 1));
        full.extend(knot_record(5, 0.5, 0.5));
        for cut in 0..full.len() {
            let _ = parse_vector_mask(&full[..cut]);
        }
    }

    #[test]
    fn subpaths_before_a_fault_are_kept() {
        let mut data = header(0);
        data.extend(length_record(0, 1, 1));
        data.extend(knot_record(1, 0.5, 0.5));
        // The second subpath promises two knots and has one.
        data.extend(length_record(0, 2, 2));
        data.extend(knot_record(1, 0.1, 0.1));
        let mask = parse_vector_mask(&data).expect("the first subpath survives");
        assert_eq!(mask.subpaths.len(), 1);
        // Four stray bytes after a whole subpath do not drop it.
        let mut data = header(0);
        data.extend(length_record(0, 1, 1));
        data.extend(knot_record(1, 0.5, 0.5));
        data.extend_from_slice(&[0; 5]);
        assert_eq!(parse_vector_mask(&data).expect("kept").subpaths.len(), 1);
    }

    #[test]
    fn the_initial_fill_record_sets_the_flag() {
        let mut data = header(0);
        let mut fill = INITIAL_FILL.to_be_bytes().to_vec();
        fill.extend_from_slice(&1u16.to_be_bytes());
        fill.resize(RECORD_SIZE, 0);
        data.extend(fill);
        assert!(parse_vector_mask(&data).expect("parses").initial_fill);
    }
}
