//! Tiled channel decoding: a viewport, its regions, and the `PackBits` reader.
//!
//! The decoder reads only the rows and columns a viewport needs. RLE rows are
//! independent, so the row-length table gives direct access to each row group.
//! The full canvas is never decoded into memory. Malformed rows return a typed
//! error.

use crate::decode_rows::{RegionGeometry, decode_raw_rows, decode_rle_rows};
use crate::error::PsdError;
use crate::limits::ParseOptions;
use crate::parse::{ChannelInfo, ColorMode, PsdVersion};
use crate::source::Source;

pub(crate) use crate::decode_rows::decode_packbits;

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
    /// Row-major samples. An 8-bit sample sits in the low byte. An empty vector
    /// is a channel with no data, and every sample of it reads as zero.
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
        if x >= self.viewport.width() || y >= self.viewport.height() {
            return None;
        }
        let index = usize::try_from(y).ok()? * usize::try_from(self.viewport.width()).ok()?
            + usize::try_from(x).ok()?;
        if channel.samples.is_empty() {
            return Some(0);
        }
        channel.samples.get(index).copied()
    }
}

/// Decode one channel over the viewport. Pixels outside the channel are zero.
///
/// With `step` above one, the output keeps the source pixels whose offset from
/// the viewport origin is a multiple of `step`, so the output is
/// `ceil(width / step)` by `ceil(height / step)`. A skipped row is never
/// decoded and the memory is the output plus one source row.
pub(crate) fn decode_channel(
    source: &Source,
    channel: &ChannelInfo,
    bytes_per_sample: usize,
    version: PsdVersion,
    viewport: Viewport,
    step: usize,
    options: &ParseOptions,
) -> Result<ChannelRegion, PsdError> {
    let step = step.max(1);
    let step32 = u32::try_from(step).unwrap_or(u32::MAX);
    let width = viewport.width().div_ceil(step32);
    let height = viewport.height().div_ceil(step32);
    let area_u64 = u64::from(width) * u64::from(height);
    let area =
        usize::try_from(area_u64).map_err(|_| PsdError::LengthTooLarge { length: area_u64 })?;

    // An empty channel block carries no sample data. Return an empty channel
    // before any read or allocation, so a short block cannot decode a neighbour
    // and a block of empty channels costs nothing. Check the compression code
    // first, so an unsupported code still errors.
    if channel.data_len == 0 {
        if channel.compression != 0 && channel.compression != 1 {
            return Err(PsdError::UnsupportedCompression(channel.compression));
        }
        return Ok(ChannelRegion {
            id: channel.id,
            samples: Vec::new(),
        });
    }
    let mut samples = vec![0u16; area];

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

    // The first kept pixel on each axis is the first one at or after the clip
    // edge whose offset from the viewport origin is a multiple of the step.
    let step_wide = i64::from(step32);
    let align = |start: i64, origin: i64| {
        start + (step_wide - (start - origin).rem_euclid(step_wide)) % step_wide
    };
    let x_first = align(x0, i64::from(viewport.left));
    let y_first = align(y0, i64::from(viewport.top));
    if x_first >= x1 || y_first >= y1 {
        return Ok(ChannelRegion {
            id: channel.id,
            samples,
        });
    }

    let geometry = RegionGeometry {
        viewport_width: usize::try_from(width).unwrap_or(0),
        step,
        first_row: usize::try_from(y_first - i64::from(bounds.top)).unwrap_or(0),
        last_row: usize::try_from(y1 - i64::from(bounds.top)).unwrap_or(0),
        source_col: usize::try_from(x_first - i64::from(bounds.left)).unwrap_or(0),
        columns: usize::try_from((x1 - x_first + step_wide - 1) / step_wide).unwrap_or(0),
        output_col: usize::try_from((x_first - i64::from(viewport.left)) / step_wide).unwrap_or(0),
        output_row: usize::try_from((y_first - i64::from(viewport.top)) / step_wide).unwrap_or(0),
        row_bytes: channel_width
            .checked_mul(bytes_per_sample)
            .ok_or(PsdError::LengthTooLarge {
                length: (channel_width as u64).saturating_mul(bytes_per_sample as u64),
            })?,
        channel_width,
        bytes_per_sample,
    };

    match channel.compression {
        0 => decode_raw_rows(source, channel, &geometry, options, &mut samples)?,
        1 => decode_rle_rows(
            source,
            channel,
            version.rle_entry_bytes(),
            &geometry,
            options,
            &mut samples,
        )?,
        code => return Err(PsdError::UnsupportedCompression(code)),
    }
    Ok(ChannelRegion {
        id: channel.id,
        samples,
    })
}

#[cfg(test)]
mod tests {
    use crate::decode::Viewport;

    #[test]
    fn a_viewport_reports_its_size() {
        let viewport = Viewport::new(10, 5, 20, 15);
        assert_eq!((viewport.width(), viewport.height()), (10, 10));
        let full = Viewport::full(64, 48);
        assert_eq!((full.width(), full.height()), (64, 48));
    }
}
