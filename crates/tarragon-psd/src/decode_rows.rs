//! Row-by-row channel decoding: read the rows a viewport needs from the source.
//!
//! A raw channel costs one read per kept row. An RLE channel costs one read of
//! the row-length table, then one read per kept row. A skipped row is never
//! read. Memory is one source row, one compressed row, and the table.

use crate::error::PsdError;
use crate::limits::ParseOptions;
use crate::parse::ChannelInfo;
use crate::source::Source;

/// The longest compressed row the decoder reads, in bytes.
///
/// A real row is at most about twice its sample bytes. A declared length above
/// this is corrupt and is refused before any allocation.
const MAX_ROW_READ: usize = 16 * 1024 * 1024;

/// The row geometry of one channel decode.
pub(crate) struct RegionGeometry {
    /// The output width in samples, which is the output row stride.
    pub(crate) viewport_width: usize,
    /// Keep every `step`th row and every `step`th column. One keeps all.
    pub(crate) step: usize,
    /// The first channel row to decode.
    pub(crate) first_row: usize,
    /// The channel row one past the last to decode.
    pub(crate) last_row: usize,
    /// The first source column inside the channel rectangle.
    pub(crate) source_col: usize,
    /// The number of source columns to copy.
    pub(crate) columns: usize,
    /// The output column of the first copied sample.
    pub(crate) output_col: usize,
    /// The output row of the first copied sample.
    pub(crate) output_row: usize,
    /// The number of bytes in a full channel row.
    pub(crate) row_bytes: usize,
    /// The channel width in pixels.
    pub(crate) channel_width: usize,
    /// The bytes per sample.
    pub(crate) bytes_per_sample: usize,
}

/// Decode the needed rows of an uncompressed channel.
///
/// Every row read is bounded to the channel data block, and the deadline is
/// checked once per row.
pub(crate) fn decode_raw_rows(
    source: &Source,
    channel: &ChannelInfo,
    geometry: &RegionGeometry,
    options: &ParseOptions,
    samples: &mut [u16],
) -> Result<(), PsdError> {
    let mut row = vec![0u16; geometry.channel_width];
    let mut bytes = vec![0u8; geometry.row_bytes];
    for (kept, row_index) in (geometry.first_row..geometry.last_row)
        .step_by(geometry.step)
        .enumerate()
    {
        options.check()?;
        let offset = channel
            .data_start
            .saturating_add((row_index as u64).saturating_mul(geometry.row_bytes as u64));
        read_block(source, channel, offset, &mut bytes)?;
        read_samples(&bytes, geometry.bytes_per_sample, &mut row);
        copy_row(&row, geometry, samples, geometry.output_row + kept);
    }
    Ok(())
}

/// Decode the needed rows of an RLE channel. Rows are independent.
pub(crate) fn decode_rle_rows(
    source: &Source,
    channel: &ChannelInfo,
    entry: usize,
    geometry: &RegionGeometry,
    options: &ParseOptions,
    samples: &mut [u16],
) -> Result<(), PsdError> {
    // The data block ends at data_start + data_len. Nothing may read past it.
    let block_end = channel.data_start.saturating_add(channel.data_len);
    // One read gives the length of every row up to the last needed row.
    let table = source.read_vec(
        channel.row_table_offset,
        geometry.last_row.saturating_mul(entry),
    )?;
    // Sum the row lengths before the first needed row to find its data.
    let mut cursor = channel.data_start;
    for row_index in 0..geometry.first_row {
        let length = row_length(&table, row_index, entry)?;
        cursor = cursor
            .checked_add(length)
            .ok_or(PsdError::LengthTooLarge { length })?;
        if cursor > block_end {
            return Err(PsdError::Truncated {
                offset: cursor,
                wanted: usize::try_from(length).unwrap_or(usize::MAX),
                remaining: 0,
            });
        }
    }
    let mut row = vec![0u16; geometry.channel_width];
    let mut scratch = vec![0u8; geometry.row_bytes];
    let mut compressed = Vec::new();
    for row_index in geometry.first_row..geometry.last_row {
        options.check()?;
        let length = row_length(&table, row_index, entry)?;
        let offset_rows = row_index - geometry.first_row;
        // A skipped row only moves the cursor past its bytes.
        if offset_rows.is_multiple_of(geometry.step) {
            let wanted = usize::try_from(length).unwrap_or(usize::MAX);
            if wanted > MAX_ROW_READ {
                return Err(PsdError::LengthTooLarge { length });
            }
            compressed.resize(wanted, 0);
            read_block(source, channel, cursor, &mut compressed)?;
            decode_packbits(
                &compressed,
                &mut scratch,
                geometry.bytes_per_sample,
                &mut row,
            );
            copy_row(
                &row,
                geometry,
                samples,
                geometry.output_row + offset_rows / geometry.step,
            );
        }
        cursor = cursor
            .checked_add(length)
            .ok_or(PsdError::LengthTooLarge { length })?;
    }
    Ok(())
}

/// Copy the needed columns of one decoded row into the output.
fn copy_row(row: &[u16], geometry: &RegionGeometry, samples: &mut [u16], output_row: usize) {
    let Some(source) = row.get(geometry.source_col..) else {
        return;
    };
    let base = output_row * geometry.viewport_width + geometry.output_col;
    let Some(target) = samples.get_mut(base..base + geometry.columns) else {
        return;
    };
    if geometry.step == 1 {
        if let Some(whole) = source.get(..geometry.columns) {
            target.copy_from_slice(whole);
        }
    } else {
        for (slot, value) in target.iter_mut().zip(source.iter().step_by(geometry.step)) {
            *slot = *value;
        }
    }
}

/// Convert one raw row of bytes into samples.
fn read_samples(bytes: &[u8], bytes_per_sample: usize, row: &mut [u16]) {
    if bytes_per_sample == 1 {
        for (slot, value) in row.iter_mut().zip(bytes.iter()) {
            *slot = u16::from(*value);
        }
    } else {
        for (slot, pair) in row.iter_mut().zip(bytes.as_chunks::<2>().0) {
            *slot = u16::from_be_bytes(*pair);
        }
    }
}

/// Decode one `PackBits` row. A short run zero-pads to the row length.
pub(crate) fn decode_packbits(
    input: &[u8],
    scratch: &mut [u8],
    bytes_per_sample: usize,
    row: &mut [u16],
) {
    scratch.fill(0);
    let row_bytes = scratch.len();
    let mut source = 0usize;
    let mut target = 0usize;
    while source < input.len() && target < row_bytes {
        let control = input[source].cast_signed();
        source += 1;
        if control >= 0 {
            let count = usize::from(control.unsigned_abs()) + 1;
            let available = input.len() - source;
            let actual = count.min(available).min(row_bytes - target);
            scratch[target..target + actual].copy_from_slice(&input[source..source + actual]);
            target += actual;
            source += count.min(available);
        } else if control != -128 {
            let Some(value) = input.get(source).copied() else {
                break;
            };
            source += 1;
            let count = usize::from(control.unsigned_abs()) + 1;
            let actual = count.min(row_bytes - target);
            scratch[target..target + actual].fill(value);
            target += actual;
        }
    }
    read_samples(scratch, bytes_per_sample, row);
}

/// Read one row-length table entry.
fn row_length(table: &[u8], index: usize, entry: usize) -> Result<u64, PsdError> {
    let offset = index.saturating_mul(entry);
    let bytes = offset
        .checked_add(entry)
        .and_then(|end| table.get(offset..end))
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

/// Fill `buf` from a place inside the channel data block.
///
/// The read is bounded to `[data_start, data_start + data_len)`. A short or
/// corrupt channel block cannot read a neighbour's bytes.
fn read_block(
    source: &Source,
    channel: &ChannelInfo,
    offset: u64,
    buf: &mut [u8],
) -> Result<(), PsdError> {
    let block_end = channel.data_start.saturating_add(channel.data_len);
    let end = offset.saturating_add(buf.len() as u64);
    if offset < channel.data_start || end > block_end {
        return Err(PsdError::Truncated {
            offset,
            wanted: buf.len(),
            remaining: usize::try_from(block_end.saturating_sub(offset)).unwrap_or(0),
        });
    }
    source.read_into(offset, buf)
}

#[cfg(test)]
mod tests {
    use crate::decode_rows::decode_packbits;

    #[test]
    fn a_literal_run_decodes() {
        // Control 2 copies the next 3 literal bytes.
        let input = [2u8, 10, 20, 30];
        let mut scratch = [0u8; 3];
        let mut row = [0u16; 3];
        decode_packbits(&input, &mut scratch, 1, &mut row);
        assert_eq!(row, [10, 20, 30]);
    }

    #[test]
    fn a_repeat_run_decodes() {
        // Control 0xFE = -2 repeats the next byte three times.
        let input = [0xFEu8, 9];
        let mut scratch = [0u8; 3];
        let mut row = [0u16; 3];
        decode_packbits(&input, &mut scratch, 1, &mut row);
        assert_eq!(row, [9, 9, 9]);
    }

    #[test]
    fn a_truncated_run_zero_pads_without_panicking() {
        // A repeat header with no value must not panic.
        let input = [0xFEu8];
        let mut scratch = [0u8; 3];
        let mut row = [0u16; 3];
        decode_packbits(&input, &mut scratch, 1, &mut row);
        assert_eq!(row, [0, 0, 0]);
    }
}
