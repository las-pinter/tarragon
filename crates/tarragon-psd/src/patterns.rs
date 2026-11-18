//! The document pattern table.
//!
//! The global additional layer info holds the patterns in a `Patt`, `Pat2` or
//! `Pat3` block. The three keys are aliases. A block is a run of records. Each
//! record has a four-byte length (padded to four bytes) and a body:
//!
//! ```text
//! version u32 (1), image mode u32, height i16, width i16,
//! name (u32 count, UTF-16BE), id (Pascal string),
//! indexed only: 768-byte palette, then 4 extra bytes unless the next u32 is 3,
//! virtual memory array list:
//!   version u32 (3), length u32, rect (top, left, bottom, right),
//!   channel count N, then N + 2 arrays:
//!     written u32, length u32, depth u32, rect, depth u16,
//!     compression u8 (0 raw, 1 RLE), data
//! ```
//!
//! The first written arrays are the color channels. The next written array is
//! the alpha. The parser indexes each record and keeps its bytes. The pixels
//! are decoded on first use. A bad record drops that pattern only.

use std::sync::atomic::AtomicUsize;
use std::sync::{Arc, OnceLock};

use crate::error::PsdError;
use crate::limits::ParseOptions;
use crate::pattern_decode::decode_tile;

/// The largest pattern edge in pixels.
const MAX_EDGE: u32 = 30_000;
/// The most pixels in one pattern.
const MAX_TILE_PIXELS: usize = 1 << 22;
/// The most decoded pixels for the patterns of one document.
pub(crate) const MAX_DECODED_PIXELS: usize = 1 << 23;
/// The most channels a pattern may declare.
const MAX_CHANNELS: u32 = 64;
/// A `PackBits` run packet makes at most 128 bytes from 2 bytes.
const MAX_RLE_EXPANSION: usize = 64;
/// The bytes of an array header after its length: depth, rect, depth, compression.
const ARRAY_HEADER: usize = 23;

/// The decoded pixels of one pattern.
pub(crate) struct PatternTile {
    /// The tile width in pixels.
    pub(crate) width: u32,
    /// The tile height in pixels.
    pub(crate) height: u32,
    /// The pixels in row order: premultiplied RGBA, 16 bits per channel.
    pub(crate) pixels: Vec<[u16; 4]>,
}

impl std::fmt::Debug for PatternTile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PatternTile")
            .field("width", &self.width)
            .field("height", &self.height)
            .finish_non_exhaustive()
    }
}

/// One pattern of the document table: its index and its record bytes.
///
/// The pixels are decoded on first use, so a pattern that no layer uses costs
/// only its record bytes.
#[derive(Clone)]
pub struct PatternEntry {
    /// The pattern name, without the trailing NUL.
    pub name: String,
    /// The unique pattern id, without the trailing NUL.
    pub id: String,
    /// The tile width in pixels.
    pub width: u32,
    /// The tile height in pixels.
    pub height: u32,
    /// The record body.
    pub(crate) record: Vec<u8>,
    /// The image mode: 1 grey, 2 indexed, 3 RGB.
    pub(crate) mode: u32,
    /// The offset of the palette in the record, for an indexed pattern.
    pub(crate) palette: Option<usize>,
    /// The offsets of the channel arrays in the record.
    pub(crate) arrays: std::ops::Range<usize>,
    /// The declared channel count.
    pub(crate) channels: u32,
    /// The pattern rect: top, left, bottom, right.
    pub(crate) bounds: [i32; 4],
    /// The decoded pixel count left for the document.
    ///
    /// A cloned file shares this budget but not the undecoded caches, so a clone can charge it again.
    pub(crate) budget: Arc<AtomicUsize>,
    /// The pixels, once decoded. `None` inside means the decode failed.
    decoded: OnceLock<Option<Arc<PatternTile>>>,
}

impl std::fmt::Debug for PatternEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PatternEntry")
            .field("name", &self.name)
            .field("id", &self.id)
            .field("width", &self.width)
            .field("height", &self.height)
            .finish_non_exhaustive()
    }
}

impl PartialEq for PatternEntry {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
            && self.id == other.id
            && self.width == other.width
            && self.height == other.height
            && self.record == other.record
    }
}

impl PatternEntry {
    /// The decoded pixels, decoded on the first call.
    ///
    /// `None` means the pattern cannot be decoded within the document budget.
    /// When the used patterns need more than the budget, the first caller wins.
    /// A refused pattern stays empty for the life of the document, so the result can depend on render order.
    pub(crate) fn tile(&self) -> Option<&PatternTile> {
        self.decoded
            .get_or_init(|| decode_tile(self).map(Arc::new))
            .as_deref()
    }

    /// True when the pixels were decoded. A pattern that no layer used is not.
    #[must_use]
    pub fn is_decoded(&self) -> bool {
        matches!(self.decoded.get(), Some(Some(_)))
    }
}

/// Find a pattern by id first, then by name.
#[must_use]
pub(crate) fn find_pattern<'a>(
    patterns: &'a [PatternEntry],
    id: &str,
    name: &str,
) -> Option<&'a PatternEntry> {
    let by_id = (!id.is_empty())
        .then(|| patterns.iter().find(|entry| entry.id == id))
        .flatten();
    by_id.or_else(|| {
        (!name.is_empty())
            .then(|| patterns.iter().find(|entry| entry.name == name))
            .flatten()
    })
}

/// A bounded big-endian reader that returns `None` instead of an error.
pub(crate) struct Cursor<'a> {
    /// The bytes.
    data: &'a [u8],
    /// The read position.
    pos: usize,
}

impl<'a> Cursor<'a> {
    /// A cursor at the start of `data`.
    pub(crate) fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    /// The bytes left.
    fn remaining(&self) -> usize {
        self.data.len() - self.pos
    }

    /// Take `count` bytes.
    fn take(&mut self, count: usize) -> Option<&'a [u8]> {
        let end = self.pos.checked_add(count)?;
        let bytes = self.data.get(self.pos..end)?;
        self.pos = end;
        Some(bytes)
    }

    /// Read one byte.
    fn u8(&mut self) -> Option<u8> {
        self.take(1).map(|bytes| bytes[0])
    }

    /// Read a `u16`.
    pub(crate) fn u16(&mut self) -> Option<u16> {
        Some(u16::from_be_bytes(self.take(2)?.try_into().ok()?))
    }

    /// Read a `u32`.
    fn u32(&mut self) -> Option<u32> {
        Some(u32::from_be_bytes(self.take(4)?.try_into().ok()?))
    }

    /// Read an `i32`.
    fn i32(&mut self) -> Option<i32> {
        Some(i32::from_be_bytes(self.take(4)?.try_into().ok()?))
    }

    /// Read a `u32` as a length.
    fn length(&mut self) -> Option<usize> {
        usize::try_from(self.u32()?).ok()
    }

    /// Read a rect: top, left, bottom, right.
    fn rect(&mut self) -> Option<[i32; 4]> {
        Some([self.i32()?, self.i32()?, self.i32()?, self.i32()?])
    }
}

/// Index the records of one `Patt`, `Pat2` or `Pat3` block into `out`.
///
/// A bad record is dropped. Only a timeout or a cancel returns an error.
/// `budget` is the decoded pixel count left for the whole document.
pub(crate) fn parse_pattern_block(
    data: &[u8],
    budget: &Arc<AtomicUsize>,
    options: &ParseOptions,
    out: &mut Vec<PatternEntry>,
) -> Result<(), PsdError> {
    let mut at = 0usize;
    while data.len().saturating_sub(at) >= 4 {
        options.check()?;
        let mut head = Cursor::new(&data[at..]);
        let Some(length) = head.length() else { break };
        let start = at + 4;
        let end = start.saturating_add(length).min(data.len());
        if let Some(entry) = index_record(&data[start..end], budget) {
            out.push(entry);
        }
        // Each record pads to a four-byte boundary.
        at = start.saturating_add(length.saturating_add(3) & !3);
    }
    Ok(())
}

/// One channel array, checked but not decoded.
pub(crate) struct Array<'a> {
    /// The bit depth, 8 or 16.
    pub(crate) depth: u16,
    /// The channel rect: top, left, bottom, right.
    pub(crate) rect: [i32; 4],
    /// The compression code: 0 raw, 1 RLE.
    pub(crate) compression: u8,
    /// The channel data.
    pub(crate) data: &'a [u8],
}

/// Read and check the channel arrays of the virtual memory array list.
///
/// This reads the color channels and one alpha. It checks the depth, the rect
/// and the input size of every array, so a decode of a checked array cannot
/// read out of range or expand without bound.
pub(crate) fn read_arrays<'a>(
    list: &mut Cursor<'a>,
    channels: u32,
    colors: usize,
    bounds: [i32; 4],
) -> Option<Vec<Array<'a>>> {
    let mut arrays = Vec::new();
    for _ in 0..channels + 2 {
        if list.remaining() == 0 || arrays.len() > colors {
            break;
        }
        if list.u32()? == 0 {
            continue;
        }
        let array_length = list.length()?;
        if array_length == 0 {
            continue;
        }
        if array_length < ARRAY_HEADER {
            return None;
        }
        let wide_depth = list.u32()?;
        let rect = list.rect()?;
        let short_depth = list.u16()?;
        let compression = list.u8()?;
        let data = list.take(array_length - ARRAY_HEADER)?;
        let depth = if matches!(short_depth, 1 | 8 | 16 | 32) {
            short_depth
        } else {
            u16::try_from(wide_depth).ok()?
        };
        // A bit or 32-bit pattern is dropped.
        if depth != 8 && depth != 16 {
            return None;
        }
        let [top, left, bottom, right] = bounds;
        if rect[0] < top
            || rect[1] < left
            || rect[2] > bottom
            || rect[3] > right
            || rect[2] < rect[0]
            || rect[3] < rect[1]
        {
            return None;
        }
        let array = Array {
            depth,
            rect,
            compression,
            data,
        };
        if !input_is_sound(&array) {
            return None;
        }
        arrays.push(array);
    }
    Some(arrays)
}

/// The width and height of a channel rect.
pub(crate) fn channel_size(rect: [i32; 4]) -> Option<(usize, usize)> {
    Some((
        usize::try_from(rect[3].checked_sub(rect[1])?).ok()?,
        usize::try_from(rect[2].checked_sub(rect[0])?).ok()?,
    ))
}

/// True when the data can fill the channel and cannot expand without bound.
///
/// This runs before any plane is allocated.
fn input_is_sound(array: &Array) -> bool {
    let Some((width, height)) = channel_size(array.rect) else {
        return false;
    };
    let bytes_per_sample = usize::from(array.depth / 8);
    let Some(total) = width
        .checked_mul(bytes_per_sample)
        .and_then(|row| row.checked_mul(height))
    else {
        return false;
    };
    let data = array.data;
    match array.compression {
        0 => data.len() >= total,
        1 => {
            let Some(table) = height.checked_mul(2).filter(|table| *table <= data.len()) else {
                return false;
            };
            let mut lengths = Cursor::new(&data[..table]);
            let mut sum = 0usize;
            while let Some(length) = lengths.u16() {
                sum += usize::from(length);
            }
            total <= data.len().saturating_mul(MAX_RLE_EXPANSION) && sum <= data.len() - table
        }
        _ => false,
    }
}

/// Read the unicode name and the Pascal id, without their trailing NULs.
fn read_name_and_id(cursor: &mut Cursor) -> Option<(String, String)> {
    let name_count = cursor.length()?;
    if name_count > 65_536 {
        return None;
    }
    let units: Vec<u16> = cursor
        .take(name_count * 2)?
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| u16::from_be_bytes(*pair))
        .collect();
    let name = String::from_utf16_lossy(&units)
        .trim_end_matches('\0')
        .to_owned();
    let id_length = usize::from(cursor.u8()?);
    let id = String::from_utf8_lossy(cursor.take(id_length)?)
        .trim_end_matches('\0')
        .to_owned();
    Some((name, id))
}

/// Index one pattern record, or `None` when it is bad or not supported.
fn index_record(record: &[u8], budget: &Arc<AtomicUsize>) -> Option<PatternEntry> {
    let mut cursor = Cursor::new(record);
    if cursor.u32()? != 1 {
        return None;
    }
    let mode = cursor.u32()?;
    // The mode sets the color channel count. CMYK, Lab, multichannel and
    // duotone patterns are not supported.
    let colors = match mode {
        1 | 2 => 1,
        3 => 3,
        _ => return None,
    };
    cursor.take(4)?;
    let (name, id) = read_name_and_id(&mut cursor)?;
    let palette = if mode == 2 {
        let at = cursor.pos;
        cursor.take(768)?;
        // Some writers add 4 bytes after the palette.
        let next = u32::from_be_bytes(
            cursor
                .data
                .get(cursor.pos..cursor.pos + 4)?
                .try_into()
                .ok()?,
        );
        if next != 3 {
            cursor.take(4)?;
        }
        Some(at)
    } else {
        None
    };
    if cursor.u32()? != 3 {
        return None;
    }
    let list_length = cursor.length()?.min(cursor.remaining());
    let list_start = cursor.pos;
    let mut list = Cursor::new(cursor.take(list_length)?);
    let [top, left, bottom, right] = list.rect()?;
    let width = u32::try_from(right.checked_sub(left)?).ok()?;
    let height = u32::try_from(bottom.checked_sub(top)?).ok()?;
    if width == 0 || height == 0 || width > MAX_EDGE || height > MAX_EDGE {
        return None;
    }
    let pixel_count = usize::try_from(width).ok()? * usize::try_from(height).ok()?;
    if pixel_count > MAX_TILE_PIXELS {
        return None;
    }
    let channels = list.u32()?;
    if channels > MAX_CHANNELS {
        return None;
    }
    let arrays = list_start + list.pos..list_start + list_length;
    let bounds = [top, left, bottom, right];
    let checked = read_arrays(&mut list, channels, colors, bounds)?;
    let depth = checked.first()?.depth;
    if checked.len() < colors
        || checked.iter().any(|array| array.depth != depth)
        || (mode == 2 && depth != 8)
    {
        return None;
    }
    Some(PatternEntry {
        name,
        id,
        width,
        height,
        record: record.to_vec(),
        mode,
        palette,
        arrays,
        channels,
        bounds,
        budget: Arc::clone(budget),
        decoded: OnceLock::new(),
    })
}
