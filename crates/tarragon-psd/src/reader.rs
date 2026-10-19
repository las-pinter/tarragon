//! Checked big-endian cursor over a byte slice.

use crate::error::PsdError;
use crate::model::PsdVersion;

/// A checked big-endian reader over a byte slice.
pub(crate) struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
    /// The file offset of `data[0]`. Nonzero for a bounded section slice.
    base: u64,
}

impl<'a> Reader<'a> {
    /// Wrap a byte slice at file offset zero.
    pub(crate) fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            pos: 0,
            base: 0,
        }
    }

    /// The current offset as a file offset.
    pub(crate) fn offset(&self) -> u64 {
        self.base + u64::try_from(self.pos).unwrap_or(u64::MAX)
    }

    /// The number of bytes left.
    pub(crate) fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.pos)
    }

    /// The total length of the wrapped slice.
    pub(crate) fn data_len(&self) -> usize {
        self.data.len()
    }

    /// Take the next `count` bytes, or fail with a typed error.
    pub(crate) fn take(&mut self, count: usize) -> Result<&'a [u8], PsdError> {
        let truncated = PsdError::Truncated {
            offset: self.offset(),
            wanted: count,
            remaining: self.remaining(),
        };
        let end = self.pos.checked_add(count).ok_or(truncated)?;
        if end > self.data.len() {
            return Err(PsdError::Truncated {
                offset: self.offset(),
                wanted: count,
                remaining: self.remaining(),
            });
        }
        let slice = &self.data[self.pos..end];
        self.pos = end;
        Ok(slice)
    }

    /// Take a bounded sub-reader. The child inherits the file offset.
    pub(crate) fn take_reader(&mut self, count: usize) -> Result<Reader<'a>, PsdError> {
        let base = self.offset();
        let slice = self.take(count)?;
        Ok(Reader {
            data: slice,
            pos: 0,
            base,
        })
    }

    /// Skip `count` bytes.
    pub(crate) fn skip(&mut self, count: usize) -> Result<(), PsdError> {
        self.take(count).map(|_| ())
    }

    /// Read a `u8`.
    pub(crate) fn read_u8(&mut self) -> Result<u8, PsdError> {
        Ok(self.take(1)?[0])
    }

    /// Read a big-endian `u16`.
    pub(crate) fn read_u16(&mut self) -> Result<u16, PsdError> {
        let bytes = self.take(2)?;
        Ok(u16::from_be_bytes([bytes[0], bytes[1]]))
    }

    /// Read a big-endian `i16`.
    pub(crate) fn read_i16(&mut self) -> Result<i16, PsdError> {
        let bytes = self.take(2)?;
        Ok(i16::from_be_bytes([bytes[0], bytes[1]]))
    }

    /// Read a big-endian `u32`.
    pub(crate) fn read_u32(&mut self) -> Result<u32, PsdError> {
        let bytes = self.take(4)?;
        Ok(u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    /// Read a big-endian `i32`.
    pub(crate) fn read_i32(&mut self) -> Result<i32, PsdError> {
        let bytes = self.take(4)?;
        Ok(i32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    /// Read a big-endian `u64`.
    pub(crate) fn read_u64(&mut self) -> Result<u64, PsdError> {
        let bytes = self.take(8)?;
        Ok(u64::from_be_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        ]))
    }

    /// Read a length field. PSD uses four bytes; PSB uses eight.
    pub(crate) fn read_length(&mut self, version: PsdVersion) -> Result<u64, PsdError> {
        match version {
            PsdVersion::Psd => Ok(u64::from(self.read_u32()?)),
            PsdVersion::Psb => self.read_u64(),
        }
    }

    /// Read a fixed-size byte array.
    pub(crate) fn read_array<const N: usize>(&mut self) -> Result<[u8; N], PsdError> {
        let bytes = self.take(N)?;
        let mut out = [0u8; N];
        out.copy_from_slice(bytes);
        Ok(out)
    }
}

/// Convert a declared length to a `usize`.
pub(crate) fn to_usize(value: u64) -> Result<usize, PsdError> {
    usize::try_from(value).map_err(|_| PsdError::LengthTooLarge { length: value })
}
