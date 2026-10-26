//! A bounded, buffered view over a [`Source`] range.
//!
//! The existing field parsers read a byte slice through [`Reader`]. A window
//! hands them a slice of just the bytes they need, read from the source on
//! demand, so a parser never holds more than one record at a time. The window
//! ends at a section bound, so a short read fails the same way it would on a
//! slice that ended there.

use crate::error::PsdError;
use crate::model::PsdVersion;
use crate::reader::Reader;
use crate::source::{MAX_READ, Source};

/// How much to read at once when walking records in order.
const READ_AHEAD: usize = 64 * 1024;
/// A buffer above this size is freed instead of kept for the next read.
const KEEP_CAPACITY: usize = 1024 * 1024;

/// A cursor over `[pos, end)` of a source.
pub(crate) struct Window<'a> {
    source: &'a Source,
    buf: Vec<u8>,
    buf_start: u64,
    pos: u64,
    end: u64,
}

impl<'a> Window<'a> {
    /// A window over `[start, end)`. The end is clamped to the source length.
    pub(crate) fn new(source: &'a Source, start: u64, end: u64) -> Self {
        Self {
            source,
            buf: Vec::new(),
            buf_start: 0,
            pos: start,
            end: end.min(source.len()),
        }
    }

    /// The source behind the window.
    pub(crate) fn source(&self) -> &'a Source {
        self.source
    }

    /// The current file offset.
    pub(crate) fn pos(&self) -> u64 {
        self.pos
    }

    /// The bytes left before the end.
    pub(crate) fn remaining(&self) -> u64 {
        self.end.saturating_sub(self.pos)
    }

    /// Move to `pos`.
    pub(crate) fn seek(&mut self, pos: u64) {
        self.pos = pos;
    }

    /// Fail with [`PsdError::Truncated`] when fewer than `count` bytes are left.
    pub(crate) fn require(&self, count: u64) -> Result<(), PsdError> {
        if count > self.remaining() {
            return Err(PsdError::Truncated {
                offset: self.pos,
                wanted: usize::try_from(count).unwrap_or(usize::MAX),
                remaining: usize::try_from(self.remaining()).unwrap_or(usize::MAX),
            });
        }
        Ok(())
    }

    /// Skip `count` bytes, or fail when they are not all inside the window.
    pub(crate) fn skip(&mut self, count: u64) -> Result<(), PsdError> {
        self.require(count)?;
        self.pos += count;
        Ok(())
    }

    /// Free the read buffer.
    pub(crate) fn release(&mut self) {
        self.buf = Vec::new();
    }

    /// The next `count` bytes, or fewer when the window ends first.
    ///
    /// With `ahead` set the window reads a larger block, so a walk over many
    /// small fields costs few reads. The position does not move.
    pub(crate) fn peek(&mut self, count: usize, ahead: bool) -> Result<&[u8], PsdError> {
        let available = self.remaining();
        let want = usize::try_from(available.min(count as u64)).unwrap_or(usize::MAX);
        if want > MAX_READ {
            return Err(PsdError::LengthTooLarge {
                length: want as u64,
            });
        }
        let held = self
            .pos
            .checked_sub(self.buf_start)
            .and_then(|from| usize::try_from(from).ok())
            .filter(|from| from.saturating_add(want) <= self.buf.len());
        let from = if let Some(from) = held {
            from
        } else {
            let fetch = if ahead {
                let block = usize::try_from(available.min(READ_AHEAD as u64)).unwrap_or(0);
                want.max(block)
            } else {
                want
            };
            if self.buf.capacity() > KEEP_CAPACITY {
                self.buf = Vec::new();
            }
            self.buf.clear();
            self.buf.resize(fetch, 0);
            self.buf_start = self.pos;
            if let Err(error) = self.source.read_into(self.pos, &mut self.buf) {
                self.buf.clear();
                return Err(error);
            }
            0
        };
        Ok(&self.buf[from..from + want])
    }

    /// Run a field parser over the next `count` bytes and move past what it
    /// consumed. On an error the position stays.
    pub(crate) fn with_reader<T>(
        &mut self,
        count: usize,
        ahead: bool,
        parse: impl FnOnce(&mut Reader) -> Result<T, PsdError>,
    ) -> Result<T, PsdError> {
        let start = self.pos;
        let slice = self.peek(count, ahead)?;
        let mut reader = Reader::at(slice, start);
        let value = parse(&mut reader)?;
        let used = reader.offset() - start;
        self.pos += used;
        Ok(value)
    }

    /// Read a big-endian `u16`.
    // A method path cannot stand in for the closure: the reader lifetime must
    // stay generic.
    #[allow(clippy::redundant_closure_for_method_calls)]
    pub(crate) fn read_u16(&mut self) -> Result<u16, PsdError> {
        // No read-ahead: the caller reads one field, then skips far ahead.
        self.with_reader(2, false, |reader| reader.read_u16())
    }

    /// Read a big-endian `i16`. It reads ahead, because the layer records
    /// follow the count at once.
    #[allow(clippy::redundant_closure_for_method_calls)]
    pub(crate) fn read_i16(&mut self) -> Result<i16, PsdError> {
        self.with_reader(2, true, |reader| reader.read_i16())
    }

    /// Read a big-endian `u32`.
    #[allow(clippy::redundant_closure_for_method_calls)]
    pub(crate) fn read_u32(&mut self) -> Result<u32, PsdError> {
        self.with_reader(4, false, |reader| reader.read_u32())
    }

    /// Read a length field. PSD uses four bytes; PSB uses eight.
    pub(crate) fn read_length(&mut self, version: PsdVersion) -> Result<u64, PsdError> {
        self.with_reader(8, false, |reader| reader.read_length(version))
    }
}
