//! Builders for vector mask (`vmsk`, `vsms`) blocks.
//!
//! A path point is `(x, y)` as a fraction of the document size. The helpers
//! write the signed 8.24 fixed-point `(y, x)` order the format uses.

/// The bytes of one path record.
const RECORD_SIZE: usize = 26;
/// The bezier handle length for a quarter circle.
const KAPPA: f64 = 0.552_284_75;
/// The flag bit that inverts the mask.
pub const VMSK_INVERT: u32 = 0x1;
/// The flag bit that disables the mask.
pub const VMSK_DISABLED: u32 = 0x4;

/// A signed 8.24 fixed-point number.
#[must_use]
pub fn fixed(value: f64) -> [u8; 4] {
    #[allow(clippy::cast_possible_truncation)]
    let raw = (value * 16_777_216.0).round() as i32;
    raw.to_be_bytes()
}

/// A length record for a subpath of `count` knots. `op` is the path operation:
/// 1 combine, 2 subtract, 3 intersect, 0 exclude.
#[must_use]
pub fn length_record(closed: bool, count: u16, op: i16) -> Vec<u8> {
    let selector: u16 = if closed { 0 } else { 3 };
    let mut record = selector.to_be_bytes().to_vec();
    record.extend_from_slice(&count.to_be_bytes());
    record.extend_from_slice(&op.to_be_bytes());
    record.resize(RECORD_SIZE, 0);
    record
}

/// A knot record with explicit control points.
#[must_use]
pub fn knot_record(
    closed: bool,
    preceding: (f64, f64),
    anchor: (f64, f64),
    leaving: (f64, f64),
) -> Vec<u8> {
    let selector: u16 = if closed { 1 } else { 4 };
    let mut record = selector.to_be_bytes().to_vec();
    for (x, y) in [preceding, anchor, leaving] {
        record.extend_from_slice(&fixed(y));
        record.extend_from_slice(&fixed(x));
    }
    record
}

/// A closed rectangle subpath: a length record and four corner knots.
#[must_use]
pub fn rect_path(op: i16, left: f64, top: f64, right: f64, bottom: f64) -> Vec<u8> {
    let mut bytes = length_record(true, 4, op);
    for corner in [(left, top), (right, top), (right, bottom), (left, bottom)] {
        bytes.extend(knot_record(true, corner, corner, corner));
    }
    bytes
}

/// A closed ellipse subpath of four curved knots.
#[must_use]
pub fn ellipse_path(op: i16, cx: f64, cy: f64, rx: f64, ry: f64) -> Vec<u8> {
    let (kx, ky) = (KAPPA * rx, KAPPA * ry);
    let mut bytes = length_record(true, 4, op);
    let knots = [
        ((cx - kx, cy - ry), (cx, cy - ry), (cx + kx, cy - ry)),
        ((cx + rx, cy - ky), (cx + rx, cy), (cx + rx, cy + ky)),
        ((cx + kx, cy + ry), (cx, cy + ry), (cx - kx, cy + ry)),
        ((cx - rx, cy + ky), (cx - rx, cy), (cx - rx, cy - ky)),
    ];
    for (preceding, anchor, leaving) in knots {
        bytes.extend(knot_record(true, preceding, anchor, leaving));
    }
    bytes
}

/// The initial fill record. `true` starts with every pixel covered.
#[must_use]
pub fn initial_fill_record(covered: bool) -> Vec<u8> {
    let mut record = 8u16.to_be_bytes().to_vec();
    record.extend_from_slice(&u16::from(covered).to_be_bytes());
    record.resize(RECORD_SIZE, 0);
    record
}

/// A whole `vmsk` block from the flag word and the path records.
#[must_use]
pub fn vmsk_block(flags: u32, parts: &[Vec<u8>]) -> Vec<u8> {
    let mut data = 3u32.to_be_bytes().to_vec();
    data.extend_from_slice(&flags.to_be_bytes());
    for part in parts {
        data.extend_from_slice(part);
    }
    data
}
