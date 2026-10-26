//! The layer info body: the layer records, then the channel image data.
//!
//! The records come first and the pixel data of every channel follows them.
//! The parser reads the records into memory one at a time and walks the
//! channel data by offset only. The pixels are read later, row by row.

use crate::error::PsdError;
use crate::layer::{
    BLEND_SIGNATURE, LayerBounds, LayerInfo, LayerRecord, read_layer_record, validate_layer_bounds,
};
use crate::limits::{MAX_LAYER_CHANNELS, ParseOptions};
use crate::model::{Compression, PsdHeader, PsdVersion};
use crate::reader::to_usize;
use crate::source::{MAX_RECORD_EXTRA, Source};
use crate::window::Window;

/// The smallest possible layer record in bytes.
const MIN_LAYER_RECORD: usize = 34;
/// The bytes of a record before the channel table: four bounds, a count.
const RECORD_HEAD: usize = 18;
/// The bytes of a record after the channel table, up to and with the extra
/// data length: signature, blend key, four single bytes, the length.
const RECORD_MID: usize = 16;

/// Parse the layer info body in the file range `[start, end)`.
pub(crate) fn parse_layer_info(
    source: &Source,
    start: u64,
    end: u64,
    header: &PsdHeader,
    options: &ParseOptions,
) -> Result<LayerInfo, PsdError> {
    let mut body = Window::new(source, start, end);
    // A negative count marks merged transparency. The absolute value is the
    // record count.
    let declared = body.read_i16()?;
    let layer_count = usize::from(declared.unsigned_abs());
    // A record needs at least 34 bytes. Reject an impossible count early.
    let needed = layer_count.saturating_mul(MIN_LAYER_RECORD);
    if needed as u64 > end.saturating_sub(start) {
        return Err(PsdError::Truncated {
            offset: body.pos(),
            wanted: needed,
            remaining: usize::try_from(body.remaining()).unwrap_or(usize::MAX),
        });
    }

    let mut layers = Vec::new();
    for _ in 0..layer_count {
        options.check()?;
        layers.push(read_record(&mut body, header, options)?);
    }
    assign_channel_offsets(&mut layers, &mut body, header.version)?;
    Ok(LayerInfo {
        layer_count,
        layers,
        merged_has_alpha: declared < 0,
    })
}

/// Read one layer record.
///
/// A peek at the fixed fields gives the record size. The whole record is then
/// read at once and parsed like a slice that ends at the body end, so a short
/// or corrupt record gives the same typed error.
fn read_record(
    body: &mut Window,
    header: &PsdHeader,
    options: &ParseOptions,
) -> Result<LayerRecord, PsdError> {
    let size = record_size(body, header.version)?;
    body.with_reader(size, true, |reader| {
        read_layer_record(reader, header, options)
    })
}

/// The number of bytes to read for the record at the window position.
///
/// A record that is bad before its extra data gets the smallest size that
/// shows the error, so a corrupt length is never read.
fn record_size(body: &mut Window, version: PsdVersion) -> Result<usize, PsdError> {
    let start = body.pos();
    let available = body.remaining();
    let head = body.peek(RECORD_HEAD, true)?;
    if head.len() < RECORD_HEAD {
        return Ok(RECORD_HEAD);
    }
    let bound =
        |at: usize| i32::from_be_bytes([head[at], head[at + 1], head[at + 2], head[at + 3]]);
    let bounds = LayerBounds {
        top: bound(0),
        left: bound(4),
        bottom: bound(8),
        right: bound(12),
    };
    let channels = usize::from(u16::from_be_bytes([head[16], head[17]]));
    if validate_layer_bounds(bounds).is_err() || channels > MAX_LAYER_CHANNELS {
        return Ok(RECORD_HEAD);
    }
    let channel_entry = 2 + if version == PsdVersion::Psb { 8 } else { 4 };
    let table = RECORD_HEAD + channels * channel_entry;
    let fixed = table + RECORD_MID;
    let mid = body.peek(fixed, true)?;
    if mid.len() < fixed || &mid[table..table + 4] != BLEND_SIGNATURE {
        return Ok(fixed);
    }
    let extra = u32::from_be_bytes([
        mid[fixed - 4],
        mid[fixed - 3],
        mid[fixed - 2],
        mid[fixed - 1],
    ]);
    // The cap comes first, so a corrupt length is refused before any read.
    if to_usize(u64::from(extra))? > MAX_RECORD_EXTRA {
        return Err(PsdError::LengthTooLarge {
            length: u64::from(extra),
        });
    }
    let total = (fixed as u64).saturating_add(u64::from(extra));
    if total > available {
        // The extra data runs past the body end.
        return Err(PsdError::Truncated {
            offset: start + fixed as u64,
            wanted: to_usize(u64::from(extra))?,
            remaining: usize::try_from(available - fixed as u64).unwrap_or(usize::MAX),
        });
    }
    to_usize(total)
}

/// Assign each layer channel its file offset inside the channel data block.
///
/// Only the two-byte compression field of each channel is read. The pixel data
/// is skipped.
fn assign_channel_offsets(
    layers: &mut [LayerRecord],
    body: &mut Window,
    version: PsdVersion,
) -> Result<(), PsdError> {
    for layer in layers.iter_mut() {
        for channel in &mut layer.channels {
            let offset = body.pos();
            channel.data_offset = offset;
            // A block shorter than the two-byte compression field is empty.
            // Do not decode it as raw, or it would read the next channel.
            if channel.length < 2 {
                if channel.length == 1 {
                    body.skip(1)?;
                }
                channel.compression = Compression::Raw.code();
                channel.data_start = offset;
                channel.data_len = 0;
                channel.row_table_offset = 0;
                continue;
            }
            let compression = body.read_u16()?;
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
            body.skip(channel.length - 2)?;
        }
    }
    Ok(())
}
