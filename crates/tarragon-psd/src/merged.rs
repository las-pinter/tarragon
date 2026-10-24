//! Extraction of the embedded merged preview image.

use crate::error::PsdError;
use crate::layer::{ChannelInfo, LayerBounds};
use crate::limits::{MAX_DIMENSION, ParseOptions};
use crate::model::{Compression, PsdHeader};
use crate::reader::Reader;

/// The merged image data section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MergedImage {
    /// The merged image compression.
    pub compression: Compression,
    /// The merged channels, in file order.
    pub channels: Vec<ChannelInfo>,
}

impl MergedImage {
    /// The merged channel for `id`, if present.
    #[must_use]
    pub fn channel(&self, id: i16) -> Option<&ChannelInfo> {
        self.channels.iter().find(|channel| channel.id == id)
    }
}

/// Read the merged image data section.
///
/// `has_alpha` is true when the layer count was negative, which makes the first
/// channel after the color channels the merged transparency.
pub(crate) fn read_merged_image(
    reader: &mut Reader,
    header: &PsdHeader,
    has_alpha: bool,
    options: &ParseOptions,
) -> Result<MergedImage, PsdError> {
    options.check()?;
    let compression = Compression::from_code(reader.read_u16()?);
    let channels = usize::from(header.channels);
    let channel_ids = merged_channel_ids(header, has_alpha);
    let bounds = canvas_bounds(header)?;
    let mut infos = Vec::new();
    match compression {
        Compression::Raw => {
            let bytes_per_sample = raw_sample_bytes(header)?;
            let row_length = u64::from(header.width) * bytes_per_sample as u64;
            let row_bytes = usize::try_from(row_length)
                .map_err(|_| PsdError::LengthTooLarge { length: row_length })?;
            let plane_length = row_length.saturating_mul(u64::from(header.height));
            let plane = row_bytes
                .checked_mul(usize::try_from(header.height).unwrap_or(usize::MAX))
                .ok_or(PsdError::LengthTooLarge {
                    length: plane_length,
                })?;
            if reader.remaining() < channels.saturating_mul(plane) {
                return Err(PsdError::Truncated {
                    offset: reader.offset(),
                    wanted: channels.saturating_mul(plane),
                    remaining: reader.remaining(),
                });
            }
            let start = reader.offset();
            for (index, id) in channel_ids.iter().enumerate() {
                let offset = start.saturating_add((index as u64).saturating_mul(plane as u64));
                infos.push(merged_channel(
                    *id,
                    offset,
                    plane as u64,
                    compression,
                    bounds,
                ));
            }
        }
        Compression::Rle => {
            let entry = header.version.rle_entry_bytes();
            let rows = usize::try_from(header.height).unwrap_or(usize::MAX);
            let entries_length = (channels as u64).saturating_mul(u64::from(header.height));
            let entries = channels.checked_mul(rows).ok_or(PsdError::LengthTooLarge {
                length: entries_length,
            })?;
            let table_length = entries_length.saturating_mul(entry as u64);
            let table_bytes = entries.checked_mul(entry).ok_or(PsdError::LengthTooLarge {
                length: table_length,
            })?;
            let table_offset = reader.offset();
            let table = reader.take(table_bytes)?;
            let available = reader.remaining();
            let mut data_start = reader.offset();
            let mut total = 0u64;
            for (index, id) in channel_ids.iter().enumerate() {
                let mut length = 0u64;
                for row in 0..rows {
                    let offset = (index * rows + row) * entry;
                    let row_length = read_row_length(table, offset, entry)?;
                    length = length
                        .checked_add(row_length)
                        .ok_or(PsdError::LengthTooLarge { length: row_length })?;
                }
                total = total
                    .checked_add(length)
                    .ok_or(PsdError::LengthTooLarge { length })?;
                let row_table_offset = table_offset
                    .saturating_add(u64::try_from(index * rows * entry).unwrap_or(u64::MAX));
                infos.push(ChannelInfo {
                    id: *id,
                    length,
                    data_offset: data_start,
                    data_start,
                    data_len: length,
                    row_table_offset,
                    compression: compression.code(),
                    bounds,
                });
                data_start = data_start
                    .checked_add(length)
                    .ok_or(PsdError::LengthTooLarge { length })?;
            }
            // The declared channel data must fit after the row table.
            if total > available as u64 {
                return Err(PsdError::Truncated {
                    offset: reader.offset(),
                    wanted: usize::try_from(total).unwrap_or(usize::MAX),
                    remaining: available,
                });
            }
        }
        other => return Err(PsdError::UnsupportedCompression(other.code())),
    }
    Ok(MergedImage {
        compression,
        channels: infos,
    })
}

/// The merged channel IDs for the color mode.
///
/// Only the first extra channel of a file with merged transparency is the alpha
/// (ID -1). Any other extra channel is a saved selection or a spot color, and
/// it keeps its position as the ID so the compositor never reads it.
fn merged_channel_ids(header: &PsdHeader, has_alpha: bool) -> Vec<i16> {
    let count = usize::from(header.channels);
    let color = header.color_mode.color_channels().unwrap_or(0);
    (0..count)
        .map(|index| {
            if index == color && has_alpha {
                -1
            } else {
                i16::try_from(index).unwrap_or(i16::MAX)
            }
        })
        .collect()
}

/// Build one merged channel description.
fn merged_channel(
    id: i16,
    offset: u64,
    length: u64,
    compression: Compression,
    bounds: LayerBounds,
) -> ChannelInfo {
    ChannelInfo {
        id,
        length,
        data_offset: offset,
        data_start: offset,
        data_len: length,
        row_table_offset: 0,
        compression: compression.code(),
        bounds,
    }
}

/// The full-canvas rectangle as layer bounds.
fn canvas_bounds(header: &PsdHeader) -> Result<LayerBounds, PsdError> {
    Ok(LayerBounds {
        top: 0,
        left: 0,
        bottom: i32::try_from(header.height).map_err(|_| PsdError::BadDimensions {
            width: header.width,
            height: header.height,
            limit: MAX_DIMENSION,
        })?,
        right: i32::try_from(header.width).map_err(|_| PsdError::BadDimensions {
            width: header.width,
            height: header.height,
            limit: MAX_DIMENSION,
        })?,
    })
}

/// The bytes per sample for the raw merged image.
fn raw_sample_bytes(header: &PsdHeader) -> Result<usize, PsdError> {
    match header.depth {
        8 => Ok(1),
        16 => Ok(2),
        _ => Err(PsdError::UnsupportedColorMode {
            mode: header.color_mode,
            depth: header.depth,
        }),
    }
}

/// Read one RLE row length from the table.
fn read_row_length(table: &[u8], offset: usize, entry: usize) -> Result<u64, PsdError> {
    let bytes = table
        .get(offset..offset + entry)
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
