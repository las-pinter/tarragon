//! ZIP channel decoding: one zlib stream per channel, with an optional delta.
//!
//! A deflate stream cannot seek, so every call inflates from the first row to
//! the last row it needs and drops the rows it does not keep. Memory is one row,
//! one input chunk, and the zlib window. Every output is bounded by one row, so
//! a hostile stream cannot grow the buffers.

use flate2::{Decompress, FlushDecompress, Status};

use crate::decode_rows::{MAX_ROW_READ, RegionGeometry, copy_row, read_block, read_samples};
use crate::error::PsdError;
use crate::limits::ParseOptions;
use crate::parse::ChannelInfo;
use crate::source::Source;

/// The most compressed bytes read from the source in one call.
const INPUT_CHUNK: usize = 64 * 1024;

/// A zlib stream read in chunks from one channel data block.
struct ZipStream<'a> {
    /// The file the channel block sits in.
    source: &'a Source,
    /// The channel whose block bounds every read.
    channel: &'a ChannelInfo,
    /// The zlib state. It holds the window and nothing else that grows.
    inflate: Decompress,
    /// The compressed bytes read last, at most [`INPUT_CHUNK`].
    chunk: Vec<u8>,
    /// The unread part of `chunk` is `chunk[start..]`.
    start: usize,
    /// The file offset of the next chunk.
    next_offset: u64,
    /// True once the stream reported its end.
    finished: bool,
    /// The deadline and cancel flag, checked once per chunk read.
    options: &'a ParseOptions,
}

impl<'a> ZipStream<'a> {
    /// Start a stream at the first byte of the channel data.
    fn new(source: &'a Source, channel: &'a ChannelInfo, options: &'a ParseOptions) -> Self {
        Self {
            source,
            channel,
            options,
            inflate: Decompress::new(true),
            chunk: Vec::new(),
            start: 0,
            next_offset: channel.data_start,
            finished: false,
        }
    }

    /// The error for bad, short, or oversized zlib data.
    fn corrupt(&self) -> PsdError {
        PsdError::CorruptChannelData {
            code: self.channel.compression,
        }
    }

    /// Read the next chunk when the last one is used up. The read stays inside
    /// the channel data block.
    ///
    /// The deadline is checked here as well as per row. A stream of empty
    /// blocks gives no output, so a row can take many chunks to fill.
    fn refill(&mut self) -> Result<(), PsdError> {
        if self.start < self.chunk.len() {
            return Ok(());
        }
        self.options.check()?;
        let block_end = self
            .channel
            .data_start
            .saturating_add(self.channel.data_len);
        let left = block_end.saturating_sub(self.next_offset);
        let wanted = usize::try_from(left).unwrap_or(usize::MAX).min(INPUT_CHUNK);
        self.chunk.resize(wanted, 0);
        self.start = 0;
        if wanted == 0 {
            return Ok(());
        }
        read_block(self.source, self.channel, self.next_offset, &mut self.chunk)?;
        self.next_offset += wanted as u64;
        Ok(())
    }

    /// Inflate into `out` once. Return the bytes made. A call that makes and
    /// reads nothing before the end is an error.
    fn step(&mut self, out: &mut [u8]) -> Result<usize, PsdError> {
        let before_in = self.inflate.total_in();
        let before_out = self.inflate.total_out();
        let status = self
            .inflate
            .decompress(&self.chunk[self.start..], out, FlushDecompress::None)
            .map_err(|_| self.corrupt())?;
        let used = usize::try_from(self.inflate.total_in() - before_in).unwrap_or(0);
        let made = usize::try_from(self.inflate.total_out() - before_out).unwrap_or(0);
        self.start = (self.start + used).min(self.chunk.len());
        self.finished = status == Status::StreamEnd;
        // The library reports a cut stream as a normal status with no progress,
        // so a call that neither reads nor writes before the end is corrupt.
        if made == 0 && used == 0 && !self.finished {
            return Err(self.corrupt());
        }
        Ok(made)
    }

    /// Fill `out` with inflated bytes.
    ///
    /// A stream that ends early, or that stops with no progress, is corrupt.
    fn read_exact(&mut self, out: &mut [u8]) -> Result<(), PsdError> {
        let mut filled = 0;
        while filled < out.len() {
            if self.finished {
                return Err(self.corrupt());
            }
            self.refill()?;
            filled += self.step(&mut out[filled..])?;
        }
        Ok(())
    }

    /// Check that the stream holds no output past the rows already read and
    /// ends with its checksum. A cut stream fails in `step`.
    fn expect_end(&mut self) -> Result<(), PsdError> {
        let mut probe = [0u8; 1];
        while !self.finished {
            self.refill()?;
            if self.step(&mut probe)? > 0 {
                return Err(self.corrupt());
            }
        }
        Ok(())
    }
}

/// Add each sample to the one before it in the row, modulo the sample size.
///
/// A 16-bit sample is a big-endian word.
fn undo_delta(row: &mut [u8], bytes_per_sample: usize) {
    if bytes_per_sample == 1 {
        let mut previous = 0u8;
        for byte in row.iter_mut() {
            previous = previous.wrapping_add(*byte);
            *byte = previous;
        }
    } else {
        let mut previous = 0u16;
        for pair in row.as_chunks_mut::<2>().0.iter_mut() {
            previous = previous.wrapping_add(u16::from_be_bytes(*pair));
            *pair = previous.to_be_bytes();
        }
    }
}

/// Decode the needed rows of a ZIP channel, with the delta when `predicted`.
///
/// The deadline is checked once per row. A row is a fixed size, so the stream
/// never inflates past the rows the channel declares: a stream with more data
/// than the full channel holds is refused when the last row is read.
pub(crate) fn decode_zip_rows(
    source: &Source,
    channel: &ChannelInfo,
    geometry: &RegionGeometry,
    predicted: bool,
    options: &ParseOptions,
    samples: &mut [u16],
) -> Result<(), PsdError> {
    let code = channel.compression;
    if !matches!(geometry.bytes_per_sample, 1 | 2) {
        return Err(PsdError::UnsupportedCompression(code));
    }
    // The parser caps the channel width, but the row buffer size still comes
    // from the file, so cap it as the RLE decoder does before allocating.
    if geometry.row_bytes > MAX_ROW_READ {
        return Err(PsdError::LengthTooLarge {
            length: geometry.row_bytes as u64,
        });
    }

    // A deflate stream cannot seek, so every tile of a ZIP channel inflates it
    // again from row 0 up to the tile's last row.
    let mut stream = ZipStream::new(source, channel, options);
    let mut bytes = vec![0u8; geometry.row_bytes];
    let mut row = vec![0u16; geometry.channel_width];
    for row_index in 0..geometry.last_row {
        options.check()?;
        stream.read_exact(&mut bytes)?;
        // A row before the first needed row, or a skipped row, only advances
        // the stream.
        let Some(offset_rows) = row_index.checked_sub(geometry.first_row) else {
            continue;
        };
        if !offset_rows.is_multiple_of(geometry.step) {
            continue;
        }
        if predicted {
            undo_delta(&mut bytes, geometry.bytes_per_sample);
        }
        read_samples(&bytes, geometry.bytes_per_sample, &mut row);
        copy_row(
            &row,
            geometry,
            samples,
            geometry.output_row + offset_rows / geometry.step,
        );
    }
    let channel_rows = usize::try_from(channel.bounds.height()).unwrap_or(0);
    if geometry.last_row >= channel_rows {
        stream.expect_end()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use flate2::Compression;
    use flate2::write::ZlibEncoder;

    use crate::decode::{Viewport, decode_channel};
    use crate::decode_zip::{ZipStream, undo_delta};
    use crate::error::PsdError;
    use crate::layer::{ChannelInfo, LayerBounds};
    use crate::limits::{Deadline, ParseOptions};
    use crate::model::PsdVersion;
    use crate::source::Source;

    const WIDTH: usize = 7;
    const HEIGHT: usize = 9;
    // The same size as the constants above, in the types the API takes.
    const BOUNDS: LayerBounds = LayerBounds {
        top: 0,
        left: 0,
        bottom: 9,
        right: 7,
    };

    /// A channel of predicted rows at the given depth, and its true samples.
    fn predicted_channel(bytes_per_sample: usize) -> (Source, ChannelInfo, Vec<u16>) {
        let samples: Vec<u16> = (0..WIDTH * HEIGHT)
            .map(|index| {
                let value = (index * 7919 + index * index * 31) % 65536;
                let value = if bytes_per_sample == 1 {
                    value % 256
                } else {
                    value
                };
                u16::try_from(value).expect("the sample fits")
            })
            .collect();
        let mut stored = Vec::new();
        for row in samples.chunks(WIDTH) {
            let mut previous = 0u16;
            for &sample in row {
                let delta = sample.wrapping_sub(previous);
                previous = sample;
                if bytes_per_sample == 1 {
                    stored.push(delta.to_le_bytes()[0]);
                } else {
                    stored.extend_from_slice(&delta.to_be_bytes());
                }
            }
        }
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(&stored).expect("the encoder takes bytes");
        let data = encoder.finish().expect("the encoder finishes");
        let channel = ChannelInfo {
            id: 0,
            length: data.len() as u64,
            data_offset: 0,
            data_start: 0,
            data_len: data.len() as u64,
            row_table_offset: 0,
            compression: 3,
            bounds: BOUNDS,
        };
        (Source::memory(data), channel, samples)
    }

    #[test]
    fn a_stepped_read_keeps_every_nth_sample_of_the_full_read() {
        for bytes_per_sample in [1usize, 2] {
            let (source, channel, samples) = predicted_channel(bytes_per_sample);
            let view = Viewport::full(7, 9);
            let options = ParseOptions::none();
            for step in [1usize, 2, 3, 4] {
                let region = decode_channel(
                    &source,
                    &channel,
                    bytes_per_sample,
                    PsdVersion::Psd,
                    view,
                    step,
                    &options,
                )
                .expect("the stepped read decodes");
                let width = WIDTH.div_ceil(step);
                let kept: Vec<u16> = (0..HEIGHT.div_ceil(step))
                    .flat_map(|y| (0..width).map(move |x| (x, y)))
                    .map(|(x, y)| samples[y * step * WIDTH + x * step])
                    .collect();
                assert_eq!(
                    region.samples, kept,
                    "{bytes_per_sample} bytes, step {step}"
                );
            }
        }
    }

    #[test]
    fn the_delta_wraps_modulo_the_sample_size() {
        let mut bytes = [250u8, 10, 250];
        undo_delta(&mut bytes, 1);
        assert_eq!(bytes, [250, 4, 254]);
        let mut words = [0xFFu8, 0xFE, 0x00, 0x05];
        undo_delta(&mut words, 2);
        assert_eq!(words, [0xFF, 0xFE, 0x00, 0x03]);
    }

    #[test]
    fn a_32bit_sample_size_is_unsupported_compression() {
        let (source, channel, _) = predicted_channel(2);
        let result = decode_channel(
            &source,
            &channel,
            4,
            PsdVersion::Psd,
            Viewport::full(7, 9),
            1,
            &ParseOptions::none(),
        );
        assert!(matches!(result, Err(PsdError::UnsupportedCompression(3))));
    }

    #[test]
    fn a_stream_of_empty_blocks_stops_at_an_expired_deadline() {
        // A zlib header, then 200 KB of empty stored blocks. They give no
        // output, so only the per-chunk check can stop the read.
        let mut data = vec![0x78, 0x01];
        for _ in 0..40_000 {
            data.extend_from_slice(&[0, 0, 0, 0xFF, 0xFF]);
        }
        let mut channel = predicted_channel(1).1;
        channel.data_len = data.len() as u64;
        let source = Source::memory(data);
        let expired =
            ParseOptions::with_deadline(Deadline::after(std::time::Duration::from_secs(0)));
        let mut stream = ZipStream::new(&source, &channel, &expired);
        let mut row = [0u8; 7];
        assert!(matches!(
            stream.read_exact(&mut row),
            Err(PsdError::Timeout)
        ));
    }
}
