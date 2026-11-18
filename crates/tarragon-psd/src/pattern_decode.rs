//! Pixel decode for the document patterns.
//!
//! A pattern record holds checked channel arrays. This module decodes them
//! into one premultiplied RGBA16 tile, within the document pixel budget.

use std::sync::atomic::Ordering;

use crate::decode::decode_packbits;
use crate::patterns::{Array, Cursor, PatternEntry, PatternTile, channel_size, read_arrays};

/// The full value of a 16-bit sample.
const FULL: u32 = 65_535;

/// Decode the pixels of an RGB, grey or indexed pattern, within the document budget.
pub(crate) fn decode_tile(entry: &PatternEntry) -> Option<PatternTile> {
    let pixel_count = usize::try_from(entry.width).ok()? * usize::try_from(entry.height).ok()?;
    entry
        .budget
        .try_update(Ordering::SeqCst, Ordering::SeqCst, |left| {
            left.checked_sub(pixel_count)
        })
        .ok()?;
    let tile = decode_pixels(entry, pixel_count);
    if tile.is_none() {
        entry.budget.fetch_add(pixel_count, Ordering::SeqCst);
    }
    tile
}

/// Decode and convert the channel planes of a pattern.
fn decode_pixels(entry: &PatternEntry, pixel_count: usize) -> Option<PatternTile> {
    let colors = match entry.mode {
        3 => 3,
        _ => 1,
    };
    let mut list = Cursor::new(entry.record.get(entry.arrays.clone())?);
    let arrays = read_arrays(&mut list, entry.channels, colors, entry.bounds)?;
    let (width, height) = (
        usize::try_from(entry.width).ok()?,
        usize::try_from(entry.height).ok()?,
    );
    let mut planes: Vec<Vec<u16>> = Vec::with_capacity(arrays.len());
    for array in &arrays {
        planes.push(decode_plane(array, entry.bounds, width, height)?);
    }
    let depth = arrays.first()?.depth;
    let palette = match entry.palette {
        Some(at) => Some(entry.record.get(at..at + 768)?),
        None => None,
    };
    let pixels = (0..pixel_count)
        .map(|index| pixel_at(&planes, colors, palette, depth, index))
        .collect();
    Some(PatternTile {
        width: entry.width,
        height: entry.height,
        pixels,
    })
}

/// Decode one array and place it in a plane of the full pattern size.
fn decode_plane(array: &Array, bounds: [i32; 4], width: usize, height: usize) -> Option<Vec<u16>> {
    let (channel_width, channel_height) = channel_size(array.rect)?;
    let samples = decode_channel(array, channel_width, channel_height)?;
    if channel_width == width && channel_height == height {
        return Some(samples);
    }
    // A smaller channel sits at its offset. The rest reads as zero.
    let offset_x = usize::try_from(array.rect[1] - bounds[1]).ok()?;
    let offset_y = usize::try_from(array.rect[0] - bounds[0]).ok()?;
    let mut full = vec![0u16; width * height];
    for row in 0..channel_height {
        let target = (offset_y + row) * width + offset_x;
        let source = row * channel_width;
        full.get_mut(target..target + channel_width)?
            .copy_from_slice(&samples[source..source + channel_width]);
    }
    Some(full)
}

/// Decode one checked channel of `width` by `height` samples, raw or RLE.
fn decode_channel(array: &Array, width: usize, height: usize) -> Option<Vec<u16>> {
    let bytes_per_sample = usize::from(array.depth / 8);
    let row_bytes = width.checked_mul(bytes_per_sample)?;
    let data = array.data;
    let mut samples = vec![0u16; width.checked_mul(height)?];
    if samples.is_empty() {
        return Some(samples);
    }
    match array.compression {
        0 => {
            let bytes = data.get(..row_bytes.checked_mul(height)?)?;
            for (row, source) in samples
                .chunks_exact_mut(width)
                .zip(bytes.chunks_exact(row_bytes))
            {
                if bytes_per_sample == 1 {
                    for (slot, value) in row.iter_mut().zip(source) {
                        *slot = u16::from(*value);
                    }
                } else {
                    for (slot, pair) in row.iter_mut().zip(source.as_chunks::<2>().0) {
                        *slot = u16::from_be_bytes(*pair);
                    }
                }
            }
        }
        1 => {
            let mut table = Cursor::new(data.get(..height.checked_mul(2)?)?);
            let mut position = height * 2;
            let mut scratch = vec![0u8; row_bytes];
            for row in samples.chunks_exact_mut(width) {
                let row_length = usize::from(table.u16()?);
                let packed = data.get(position..position.checked_add(row_length)?)?;
                decode_packbits(packed, &mut scratch, bytes_per_sample, row);
                position += row_length;
            }
        }
        _ => return None,
    }
    Some(samples)
}

/// One sample as a 16-bit value.
fn wide(sample: u16, depth: u16) -> u32 {
    if depth == 8 {
        u32::from(sample) * 257
    } else {
        u32::from(sample)
    }
}

/// One premultiplied 16-bit RGBA pixel from the planes.
fn pixel_at(
    planes: &[Vec<u16>],
    colors: usize,
    palette: Option<&[u8]>,
    depth: u16,
    index: usize,
) -> [u16; 4] {
    let sample = |plane: usize| wide(planes[plane][index], depth);
    let rgb = if let Some(palette) = palette {
        let entry = usize::from(planes[0][index]).min(255) * 3;
        [
            u32::from(palette[entry]) * 257,
            u32::from(palette[entry + 1]) * 257,
            u32::from(palette[entry + 2]) * 257,
        ]
    } else if colors == 3 {
        [sample(0), sample(1), sample(2)]
    } else {
        let grey = sample(0);
        [grey, grey, grey]
    };
    let alpha = if planes.len() > colors {
        sample(colors)
    } else {
        FULL
    };
    let premultiply =
        |value: u32| u16::try_from((value * alpha + FULL / 2) / FULL).unwrap_or(u16::MAX);
    [
        premultiply(rgb[0]),
        premultiply(rgb[1]),
        premultiply(rgb[2]),
        u16::try_from(alpha).unwrap_or(u16::MAX),
    ]
}
