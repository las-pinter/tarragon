//! Read the image resources that the effect paint needs.
//!
//! The section holds `8BIM` blocks: a signature, a two-byte id, a padded
//! Pascal name, a four-byte length, the data, and one even-byte pad. Only the
//! two global light ids are read here; every other block is skipped.
//!
//! A short block ends the walk and keeps the values read so far. A block with
//! another signature is skipped, because only an `8BIM` block carries a value
//! this reader knows.

use crate::effects::GlobalLight;
use crate::error::PsdError;
use crate::reader::{Reader, to_usize};
use crate::window::Window;

/// The resource signature.
const SIGNATURE: &[u8; 4] = b"8BIM";
/// The global light angle id.
const GLOBAL_ANGLE_ID: u16 = 1037;
/// The global light altitude id.
const GLOBAL_ALTITUDE_ID: u16 = 1038;

/// The most a block head can take: signature, id, a 255 byte name with its
/// length byte and pad, and the data length.
const MAX_HEAD: usize = 4 + 2 + 1 + 255 + 1 + 4;

/// Read the document global light from the image resources section.
///
/// The window covers the section. Each block is walked by its head and its
/// data is skipped by seeking, so a large block that holds no light value is
/// never read. A missing id keeps the default value, so a file without the
/// resources paints the same way Photoshop does.
///
/// # Errors
///
/// Only a failed read of the source is an error. A malformed block ends the
/// walk and keeps the values read so far.
pub(crate) fn read_global_light(window: &mut Window) -> Result<GlobalLight, PsdError> {
    let mut light = GlobalLight::default();
    while window.remaining() >= 12 {
        let count = to_usize(window.remaining().min(MAX_HEAD as u64))?;
        let start = window.pos();
        let slice = window.peek(count, true)?;
        let mut reader = Reader::at(slice, start);
        let Ok((known, id, size)) = read_head(&mut reader) else {
            break;
        };
        let used = reader.offset() - start;
        window.skip(used)?;
        if window.remaining() < size {
            break;
        }
        if known && (id == GLOBAL_ANGLE_ID || id == GLOBAL_ALTITUDE_ID) && size >= 4 {
            let degrees = degrees_of(window.peek(4, false)?);
            if id == GLOBAL_ANGLE_ID {
                light.angle = degrees;
            } else {
                light.altitude = degrees;
            }
        }
        window.skip(size)?;
        // The data block is padded to an even length.
        if size % 2 == 1 && window.skip(1).is_err() {
            break;
        }
    }
    Ok(light)
}

/// Read a block head: whether the signature is known, the id, the data length.
fn read_head(reader: &mut Reader<'_>) -> Result<(bool, u16, u64), PsdError> {
    let known = &reader.read_array::<4>()? == SIGNATURE;
    let id = reader.read_u16()?;
    let size = read_block_head(reader)?;
    Ok((known, id, u64::from(size)))
}

/// Read the padded Pascal name and the data length that follows it.
fn read_block_head(reader: &mut Reader<'_>) -> Result<u32, PsdError> {
    let name_length = usize::from(reader.read_u8()?);
    reader.skip(name_length)?;
    // The length byte plus the name round up to an even count.
    if name_length % 2 == 0 {
        reader.skip(1)?;
    }
    reader.read_u32()
}

/// Read four big-endian bytes as a whole-degree value.
#[allow(
    clippy::cast_precision_loss,
    reason = "an angle in whole degrees is far below the f32 exact range"
)]
fn degrees_of(raw: &[u8]) -> f32 {
    i32::from_be_bytes([raw[0], raw[1], raw[2], raw[3]]) as f32
}
