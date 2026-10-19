//! Tiled channel decoding: a viewport, its regions, and the `PackBits` reader.
//!
//! The decoder reads only the rows and columns a viewport needs. RLE rows are
//! independent, so the row-length table gives direct access to each row group.
//! The full canvas is never decoded into memory. No input can panic.

use crate::error::PsdError;
use crate::limits::ParseOptions;
use crate::parse::{ChannelInfo, ColorMode, PsdVersion};

/// A viewport rectangle in canvas pixels. The right and bottom edges are
/// exclusive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Viewport {
    /// The left edge.
    pub left: u32,
    /// The top edge.
    pub top: u32,
    /// The right edge, exclusive.
    pub right: u32,
    /// The bottom edge, exclusive.
    pub bottom: u32,
}

impl Viewport {
    /// Build a viewport from its edges.
    #[must_use]
    pub const fn new(left: u32, top: u32, right: u32, bottom: u32) -> Self {
        Self {
            left,
            top,
            right,
            bottom,
        }
    }

    /// The full canvas viewport.
    #[must_use]
    pub const fn full(width: u32, height: u32) -> Self {
        Self::new(0, 0, width, height)
    }

    /// The viewport width in pixels.
    #[must_use]
    pub const fn width(&self) -> u32 {
        self.right.saturating_sub(self.left)
    }

    /// The viewport height in pixels.
    #[must_use]
    pub const fn height(&self) -> u32 {
        self.bottom.saturating_sub(self.top)
    }
}

/// The decoded samples of one channel over a viewport.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChannelRegion {
    /// The channel ID.
    pub id: i16,
    /// Row-major samples. An 8-bit sample sits in the low byte.
    pub samples: Vec<u16>,
}

/// The decoded channels of one image region.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageRegion {
    /// The decoded viewport.
    pub viewport: Viewport,
    /// The bit depth per channel.
    pub depth: u16,
    /// The file color mode.
    pub color_mode: ColorMode,
    /// The decoded channels, in file order.
    pub channels: Vec<ChannelRegion>,
}

impl ImageRegion {
    /// The sample at `(x, y)` for the channel with `id`.
    #[must_use]
    pub fn sample(&self, id: i16, x: u32, y: u32) -> Option<u16> {
        let channel = self.channels.iter().find(|channel| channel.id == id)?;
        let index = usize::try_from(y).ok()? * usize::try_from(self.viewport.width()).ok()?
            + usize::try_from(x).ok()?;
        channel.samples.get(index).copied()
    }
}

/// The row geometry of one channel decode.
struct RegionGeometry {
    /// The viewport width, the output row stride.
    viewport_width: usize,
    /// The first channel row to decode.
    first_row: usize,
    /// The channel row one past the last to decode.
    last_row: usize,
    /// The first source column inside the channel rectangle.
    source_col: usize,
    /// The number of source columns to copy.
    columns: usize,
    /// The output column of the first copied sample.
    output_col: usize,
    /// The output row of the first copied sample.
    output_row: usize,
    /// The number of bytes in a full channel row.
    row_bytes: usize,
    /// The channel width in pixels.
    channel_width: usize,
    /// The bytes per sample.
    bytes_per_sample: usize,
}

/// Decode one channel over the viewport. Pixels outside the channel are zero.
pub(crate) fn decode_channel(
    data: &[u8],
    channel: &ChannelInfo,
    bytes_per_sample: usize,
    version: PsdVersion,
    viewport: Viewport,
    options: &ParseOptions,
) -> Result<ChannelRegion, PsdError> {
    let width = viewport.width();
    let height = viewport.height();
    let area = usize::try_from(u64::from(width) * u64::from(height)).map_err(|_| {
        PsdError::LengthTooLarge {
            length: u64::from(width) * u64::from(height),
        }
    })?;
    let mut samples = vec![0u16; area];

    // An empty channel block carries no sample data. Return
    // zeros before any read, so a short block cannot decode a neighbour. Check
    // the compression code first, so an unsupported code still errors.
    if channel.data_len == 0 {
        if channel.compression != 0 && channel.compression != 1 {
            return Err(PsdError::UnsupportedCompression(channel.compression));
        }
        return Ok(ChannelRegion {
            id: channel.id,
            samples,
        });
    }

    let bounds = channel.bounds;
    let Ok(channel_width) = usize::try_from(bounds.width()) else {
        return Ok(ChannelRegion {
            id: channel.id,
            samples,
        });
    };
    if channel_width == 0 {
        return Ok(ChannelRegion {
            id: channel.id,
            samples,
        });
    }

    // Clip the viewport to the channel rectangle.
    let x0 = i64::from(viewport.left).max(i64::from(bounds.left));
    let x1 = i64::from(viewport.right).min(i64::from(bounds.right));
    let y0 = i64::from(viewport.top).max(i64::from(bounds.top));
    let y1 = i64::from(viewport.bottom).min(i64::from(bounds.bottom));
    if x1 <= x0 || y1 <= y0 {
        return Ok(ChannelRegion {
            id: channel.id,
            samples,
        });
    }

    let geometry = RegionGeometry {
        viewport_width: usize::try_from(width).unwrap_or(0),
        first_row: usize::try_from(y0 - i64::from(bounds.top)).unwrap_or(0),
        last_row: usize::try_from(y1 - i64::from(bounds.top)).unwrap_or(0),
        source_col: usize::try_from(x0 - i64::from(bounds.left)).unwrap_or(0),
        columns: usize::try_from(x1 - x0).unwrap_or(0),
        output_col: usize::try_from(x0 - i64::from(viewport.left)).unwrap_or(0),
        output_row: usize::try_from(y0 - i64::from(viewport.top)).unwrap_or(0),
        row_bytes: channel_width
            .checked_mul(bytes_per_sample)
            .ok_or(PsdError::LengthTooLarge { length: 0 })?,
        channel_width,
        bytes_per_sample,
    };

    match channel.compression {
        0 => decode_raw_rows(data, channel, &geometry, options, &mut samples)?,
        1 => decode_rle_rows(data, channel, version, &geometry, options, &mut samples)?,
        code => return Err(PsdError::UnsupportedCompression(code)),
    }
    Ok(ChannelRegion {
        id: channel.id,
        samples,
    })
}

/// Decode the needed rows of an uncompressed channel.
///
/// Every row read is bounded to the channel data block, and the deadline is
/// checked once per row.
fn decode_raw_rows(
    data: &[u8],
    channel: &ChannelInfo,
    geometry: &RegionGeometry,
    options: &ParseOptions,
    samples: &mut [u16],
) -> Result<(), PsdError> {
    let mut row = vec![0u16; geometry.channel_width];
    for row_index in geometry.first_row..geometry.last_row {
        options.check()?;
        let offset = channel
            .data_start
            .saturating_add((row_index as u64).saturating_mul(geometry.row_bytes as u64));
        let bytes = block_slice(data, channel, offset, geometry.row_bytes)?;
        read_samples(bytes, geometry.bytes_per_sample, &mut row);
        copy_row(
            &row,
            geometry,
            samples,
            geometry.output_row + (row_index - geometry.first_row),
        );
    }
    Ok(())
}

/// Decode the needed rows of an RLE channel. Rows are independent.
fn decode_rle_rows(
    data: &[u8],
    channel: &ChannelInfo,
    version: PsdVersion,
    geometry: &RegionGeometry,
    options: &ParseOptions,
    samples: &mut [u16],
) -> Result<(), PsdError> {
    let entry = version.rle_entry_bytes();
    // The data block ends at data_start + data_len. Nothing may read past it.
    let block_end = channel.data_start.saturating_add(channel.data_len);
    // Sum the row lengths before the first needed row to find its data.
    let mut cursor = channel.data_start;
    for row_index in 0..geometry.first_row {
        let length = read_row_length(data, channel.row_table_offset, row_index, entry)?;
        cursor = cursor
            .checked_add(length)
            .ok_or(PsdError::LengthTooLarge { length: cursor })?;
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
    for row_index in geometry.first_row..geometry.last_row {
        options.check()?;
        let length = read_row_length(data, channel.row_table_offset, row_index, entry)?;
        let compressed = block_slice(data, channel, cursor, usize::try_from(length).unwrap_or(0))?;
        decode_packbits(
            compressed,
            &mut scratch,
            geometry.bytes_per_sample,
            &mut row,
        );
        copy_row(
            &row,
            geometry,
            samples,
            geometry.output_row + (row_index - geometry.first_row),
        );
        cursor = cursor
            .checked_add(length)
            .ok_or(PsdError::LengthTooLarge { length: cursor })?;
    }
    Ok(())
}

/// Copy the needed columns of one decoded row into the output.
fn copy_row(row: &[u16], geometry: &RegionGeometry, samples: &mut [u16], output_row: usize) {
    let Some(source) = row.get(geometry.source_col..geometry.source_col + geometry.columns) else {
        return;
    };
    let base = output_row * geometry.viewport_width + geometry.output_col;
    if let Some(target) = samples.get_mut(base..base + geometry.columns) {
        target.copy_from_slice(source);
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
fn decode_packbits(input: &[u8], scratch: &mut [u8], bytes_per_sample: usize, row: &mut [u16]) {
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
fn read_row_length(
    data: &[u8],
    table_offset: u64,
    index: usize,
    entry: usize,
) -> Result<u64, PsdError> {
    let offset = table_offset.saturating_add((index as u64).saturating_mul(entry as u64));
    let bytes = slice_at(data, offset, entry)?;
    Ok(match entry {
        2 => u64::from(u16::from_be_bytes([bytes[0], bytes[1]])),
        4 => u64::from(u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])),
        _ => 0,
    })
}

/// Take a checked byte slice at a file offset.
fn slice_at(data: &[u8], offset: u64, length: usize) -> Result<&[u8], PsdError> {
    let truncated = PsdError::Truncated {
        offset,
        wanted: length,
        remaining: 0,
    };
    let start = usize::try_from(offset).map_err(|_| truncated)?;
    let remaining = data.len().saturating_sub(start);
    let end = start.checked_add(length).ok_or(PsdError::Truncated {
        offset,
        wanted: length,
        remaining,
    })?;
    data.get(start..end).ok_or(PsdError::Truncated {
        offset,
        wanted: length,
        remaining,
    })
}

/// Take a checked slice inside the channel data block.
///
/// The read is bounded to `[data_start, data_start + data_len)`. A short or
/// corrupt channel block cannot read a neighbour's bytes.
fn block_slice<'a>(
    data: &'a [u8],
    channel: &ChannelInfo,
    offset: u64,
    length: usize,
) -> Result<&'a [u8], PsdError> {
    let block_end = channel.data_start.saturating_add(channel.data_len);
    let end = offset.saturating_add(u64::try_from(length).unwrap_or(u64::MAX));
    if offset < channel.data_start || end > block_end {
        return Err(PsdError::Truncated {
            offset,
            wanted: length,
            remaining: usize::try_from(block_end.saturating_sub(offset)).unwrap_or(0),
        });
    }
    slice_at(data, offset, length)
}

#[cfg(test)]
mod tests {
    use crate::decode::{Viewport, decode_packbits};

    #[test]
    fn a_viewport_reports_its_size() {
        let viewport = Viewport::new(10, 5, 20, 15);
        assert_eq!((viewport.width(), viewport.height()), (10, 10));
        let full = Viewport::full(64, 48);
        assert_eq!((full.width(), full.height()), (64, 48));
    }

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
