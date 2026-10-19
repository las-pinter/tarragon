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
use crate::reader::{Reader, to_usize};

/// The resource signature.
const SIGNATURE: &[u8; 4] = b"8BIM";
/// The global light angle id.
const GLOBAL_ANGLE_ID: u16 = 1037;
/// The global light altitude id.
const GLOBAL_ALTITUDE_ID: u16 = 1038;

/// Read the document global light from the image resources section.
///
/// A missing id keeps the default value, so a file without the resources
/// paints the same way Photoshop does.
#[must_use]
pub(crate) fn parse_global_light(data: &[u8]) -> GlobalLight {
    let mut light = GlobalLight::default();
    let mut reader = Reader::new(data);
    while reader.remaining() >= 12 {
        let Ok(signature) = reader.read_array::<4>() else {
            break;
        };
        let known = &signature == SIGNATURE;
        let Ok(id) = reader.read_u16() else {
            break;
        };
        let Ok(size) = read_block_head(&mut reader) else {
            break;
        };
        let size = to_usize(u64::from(size)).unwrap_or(0);
        let Ok(value) = reader.take(size) else {
            break;
        };
        if known
            && (id == GLOBAL_ANGLE_ID || id == GLOBAL_ALTITUDE_ID)
            && let Some(raw) = value.get(..4)
        {
            let degrees = degrees_of(raw);
            if id == GLOBAL_ANGLE_ID {
                light.angle = degrees;
            } else {
                light.altitude = degrees;
            }
        }
        // The data block is padded to an even length.
        if size % 2 == 1 && reader.skip(1).is_err() {
            break;
        }
    }
    light
}

/// Read the padded Pascal name and the data length that follows it.
fn read_block_head(reader: &mut Reader<'_>) -> Result<u32, crate::error::PsdError> {
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
