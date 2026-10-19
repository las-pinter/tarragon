//! Minimal read-only reader for the SQLite page format.
//!
//! A `.clip` document embeds a SQLite database after a binary header. The
//! reader seeks to that byte offset in one open file and reads whole pages on
//! demand. It follows the table b-tree to the preview row and follows overflow
//! pages for a large blob. The document never enters memory as a whole.
//!
//! The reader covers the subset the preview needs: table b-tree pages, records,
//! overflow chains, and the `sqlite_master` schema. It does not write, and it
//! does not read index pages or free pages.

use std::io::{Read, Seek, SeekFrom};

use super::record::{Table, parse_record, table_from_schema};

/// The 16-byte magic at the start of a SQLite database.
const HEADER_MAGIC: &[u8; 16] = b"SQLite format 3\0";
/// The database header length in bytes.
const HEADER_BYTES: u64 = 100;
/// The smallest legal page size.
const MIN_PAGE_SIZE: u32 = 512;
/// The largest legal page size.
const MAX_PAGE_SIZE: u32 = 65_536;
/// The smallest legal usable page size.
const MIN_USABLE_SIZE: u32 = 480;
/// The payload margin of a table b-tree page.
const TABLE_PAYLOAD_MARGIN: u64 = 35;
/// The byte count of the overflow page pointer in a cell.
const OVERFLOW_POINTER_BYTES: u64 = 4;
/// The byte count of the child page pointer in an interior cell.
const CHILD_POINTER_BYTES: u64 = 4;
/// The freeblock pointer length in a page header.
const PAGE_HEADER_LENGTH: usize = 8;
/// The largest cell payload the reader will allocate.
const MAX_PAYLOAD_BYTES: u64 = 64 * 1024 * 1024;

/// An error from the embedded database reader.
#[derive(Debug)]
pub(super) enum DbError {
    /// The file could not be read.
    Io(std::io::Error),
    /// The bytes do not follow the SQLite page format.
    Format(String),
}

impl From<std::io::Error> for DbError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

/// An open embedded database inside a larger file.
pub(super) struct Database<R> {
    source: R,
    base: u64,
    page_size: u32,
    usable: u32,
    page_count: u32,
    file_pages: u32,
}

impl<R: Read + Seek> Database<R> {
    /// Open the database whose header starts at `base`.
    pub(super) fn open(mut source: R, base: u64) -> Result<Self, DbError> {
        source.seek(SeekFrom::Start(base))?;
        let mut header = [0u8; 100];
        read_exact(&mut source, &mut header)?;
        if &header[..HEADER_MAGIC.len()] != HEADER_MAGIC {
            return Err(DbError::Format(
                "the embedded database has no SQLite header".to_owned(),
            ));
        }
        let raw_page_size = be_u16(&header, 16)?;
        let page_size = if raw_page_size == 1 {
            65_536
        } else {
            u32::from(raw_page_size)
        };
        if !(MIN_PAGE_SIZE..=MAX_PAGE_SIZE).contains(&page_size) || !page_size.is_power_of_two() {
            return Err(DbError::Format(format!(
                "the embedded database has a bad page size {page_size}"
            )));
        }
        let reserved = u32::from(header[20]);
        if reserved >= page_size {
            return Err(DbError::Format(
                "the embedded database reserves too many page bytes".to_owned(),
            ));
        }
        let usable = page_size - reserved;
        if usable < MIN_USABLE_SIZE {
            return Err(DbError::Format(
                "the embedded database page is too small".to_owned(),
            ));
        }
        let page_count = be_u32(&header, 28)?;
        let end = source.seek(SeekFrom::End(0))?;
        let file_bytes = end.saturating_sub(base);
        let file_pages = u32::try_from(file_bytes / u64::from(page_size)).unwrap_or(u32::MAX);
        if file_pages == 0 {
            return Err(DbError::Format(
                "the embedded database has no complete page".to_owned(),
            ));
        }
        Ok(Self {
            source,
            base,
            page_size,
            usable,
            page_count,
            file_pages,
        })
    }

    /// The highest page number the file can hold.
    fn page_limit(&self) -> u32 {
        if self.page_count == 0 {
            self.file_pages
        } else {
            self.page_count.min(self.file_pages)
        }
    }

    /// Read one whole page. Page numbers start at 1.
    fn read_page(&mut self, page_number: u32) -> Result<Vec<u8>, DbError> {
        if page_number == 0 || page_number > self.page_limit() {
            return Err(DbError::Format(format!(
                "page {page_number} is outside the embedded database"
            )));
        }
        let distance = u64::from(page_number - 1)
            .checked_mul(u64::from(self.page_size))
            .ok_or_else(|| DbError::Format("the page offset overflows".to_owned()))?;
        let offset = self
            .base
            .checked_add(distance)
            .ok_or_else(|| DbError::Format("the page offset overflows".to_owned()))?;
        self.source.seek(SeekFrom::Start(offset))?;
        let mut page = vec![0u8; self.page_size as usize];
        read_exact(&mut self.source, &mut page)?;
        Ok(page)
    }

    /// Find a table in the schema b-tree.
    pub(super) fn lookup_table(&mut self, name: &str) -> Result<Option<Table>, DbError> {
        let mut stack = vec![1_u32];
        let mut visited = 0_u32;
        let limit = self.page_limit();
        while let Some(number) = stack.pop() {
            visited += 1;
            if visited > limit {
                return Err(DbError::Format("the schema b-tree has a loop".to_owned()));
            }
            let page = self.read_page(number)?;
            let header = page_header(&page, number, self.usable)?;
            match header.kind {
                PageKind::LeafTable => {
                    for index in 0..header.cell_count {
                        let offset = cell_offset(&page, &header, index)?;
                        let payload = self.leaf_payload(&page, offset)?;
                        let values = parse_record(&payload)?;
                        if let Some(table) = table_from_schema(&values, name)? {
                            return Ok(Some(table));
                        }
                    }
                }
                PageKind::InteriorTable => {
                    for index in 0..header.cell_count {
                        let offset = cell_offset(&page, &header, index)?;
                        stack.push(be_u32(&page, offset)?);
                    }
                    if let Some(right_most) = header.right_most {
                        stack.push(right_most);
                    }
                }
            }
        }
        Ok(None)
    }

    /// Read the payload of the first row of a table b-tree.
    ///
    /// This is the row order a plain `SELECT ... LIMIT 1` sees: the leftmost
    /// leaf holds the smallest rowid.
    pub(super) fn first_row(&mut self, root_page: u32) -> Result<Option<Vec<u8>>, DbError> {
        let mut number = root_page;
        let mut depth = 0_u32;
        let limit = self.page_limit();
        loop {
            depth += 1;
            if depth > limit + 1 {
                return Err(DbError::Format("the table b-tree has a loop".to_owned()));
            }
            let page = self.read_page(number)?;
            let header = page_header(&page, number, self.usable)?;
            match header.kind {
                PageKind::InteriorTable => {
                    number = if header.cell_count == 0 {
                        header.right_most.ok_or_else(|| {
                            DbError::Format("an interior page has no child".to_owned())
                        })?
                    } else {
                        let offset = cell_offset(&page, &header, 0)?;
                        be_u32(&page, offset)?
                    };
                }
                PageKind::LeafTable => {
                    if header.cell_count == 0 {
                        return Ok(None);
                    }
                    let offset = cell_offset(&page, &header, 0)?;
                    return Ok(Some(self.leaf_payload(&page, offset)?));
                }
            }
        }
    }

    /// Read one table leaf cell payload, following the overflow chain.
    fn leaf_payload(&mut self, page: &[u8], cell: usize) -> Result<Vec<u8>, DbError> {
        let (payload_len, length_bytes) = read_varint(page, cell)?;
        let (_rowid, rowid_bytes) = read_varint(page, cell + length_bytes)?;
        let start = cell
            .checked_add(length_bytes)
            .and_then(|value| value.checked_add(rowid_bytes))
            .ok_or_else(|| DbError::Format("the cell offset overflows".to_owned()))?;
        let local_len = local_payload_len(payload_len, self.usable);
        let local_end = start
            .checked_add(usize::try_from(local_len).unwrap_or(usize::MAX))
            .ok_or_else(|| DbError::Format("the cell offset overflows".to_owned()))?;
        let local = page
            .get(start..local_end)
            .ok_or_else(|| DbError::Format("the cell payload is cut".to_owned()))?;
        check_payload_len(payload_len)?;
        let limit = u64::from(self.page_limit()).saturating_mul(u64::from(self.usable));
        if payload_len > limit {
            return Err(DbError::Format(
                "the cell payload is larger than the database".to_owned(),
            ));
        }
        let mut payload = Vec::with_capacity(usize::try_from(payload_len).unwrap_or(0));
        payload.extend_from_slice(local);
        if payload_len > local_len {
            let mut next = be_u32(page, local_end)?;
            let mut remaining = payload_len - local_len;
            let per_page = u64::from(self.usable) - OVERFLOW_POINTER_BYTES;
            let guard = remaining.div_ceil(per_page);
            let mut steps = 0_u64;
            while remaining > 0 {
                if next == 0 {
                    return Err(DbError::Format("the overflow chain ends early".to_owned()));
                }
                steps += 1;
                if steps > guard {
                    return Err(DbError::Format(
                        "the overflow chain does not shrink".to_owned(),
                    ));
                }
                let overflow = self.read_page(next)?;
                let first = usize::try_from(OVERFLOW_POINTER_BYTES).unwrap_or(0);
                let last = usize::try_from(OVERFLOW_POINTER_BYTES + per_page).unwrap_or(0);
                let chunk = overflow
                    .get(first..last)
                    .ok_or_else(|| DbError::Format("the overflow page is cut".to_owned()))?;
                let take = usize::try_from(remaining.min(per_page)).unwrap_or(0);
                payload.extend_from_slice(&chunk[..take]);
                remaining -= u64::try_from(take).unwrap_or(0);
                next = be_u32(&overflow, 0)?;
            }
        }
        Ok(payload)
    }
}

/// The page kind this reader can follow.
#[derive(Clone, Copy)]
enum PageKind {
    /// A table b-tree interior page.
    InteriorTable,
    /// A table b-tree leaf page.
    LeafTable,
}

/// The parsed b-tree page header.
struct PageHeader {
    kind: PageKind,
    cell_count: usize,
    cell_pointers: usize,
    content_start: usize,
    right_most: Option<u32>,
}

/// Parse the b-tree header of one page.
fn page_header(page: &[u8], page_number: u32, usable: u32) -> Result<PageHeader, DbError> {
    let offset = if page_number == 1 {
        usize::try_from(HEADER_BYTES).unwrap_or(0)
    } else {
        0
    };
    let kind = match page.get(offset).copied() {
        Some(5) => PageKind::InteriorTable,
        Some(13) => PageKind::LeafTable,
        Some(2 | 10) => {
            return Err(DbError::Format(
                "an index page holds the requested table".to_owned(),
            ));
        }
        Some(other) => {
            return Err(DbError::Format(format!("unknown b-tree page type {other}")));
        }
        None => {
            return Err(DbError::Format("the page has no b-tree header".to_owned()));
        }
    };
    let cell_count = usize::from(be_u16(page, offset + 3)?);
    let raw_content_start = usize::from(be_u16(page, offset + 5)?);
    let header_length = PAGE_HEADER_LENGTH;
    let (cell_pointers, right_most) = match kind {
        PageKind::InteriorTable => (
            offset + header_length + usize::try_from(CHILD_POINTER_BYTES).unwrap_or(0),
            Some(be_u32(page, offset + header_length)?),
        ),
        PageKind::LeafTable => (offset + header_length, None),
    };
    let pointer_end = cell_pointers + cell_count * 2;
    if pointer_end > page.len() {
        return Err(DbError::Format(
            "the cell pointer array leaves the page".to_owned(),
        ));
    }
    let content_start = if raw_content_start == 0 {
        page.len()
    } else {
        raw_content_start
    };
    let usable_bytes = usize::try_from(usable).unwrap_or(page.len());
    if content_start > page.len().min(usable_bytes) {
        return Err(DbError::Format(
            "the cell content start leaves the page".to_owned(),
        ));
    }
    // The cell pointer array ends where the cell content area can start. Page 1
    // keeps its b-tree header after the 100-byte database header, so this floor
    // also rejects a pointer inside that header.
    if content_start < pointer_end {
        return Err(DbError::Format(
            "the cell content start overlaps the cell pointer array".to_owned(),
        ));
    }
    Ok(PageHeader {
        kind,
        cell_count,
        cell_pointers,
        content_start,
        right_most,
    })
}

/// Return the absolute page offset of one cell.
fn cell_offset(page: &[u8], header: &PageHeader, index: usize) -> Result<usize, DbError> {
    if index >= header.cell_count {
        return Err(DbError::Format("the cell index leaves the page".to_owned()));
    }
    let pointer_at = header.cell_pointers + index * 2;
    let offset = usize::from(be_u16(page, pointer_at)?);
    if offset < header.content_start || offset >= page.len() {
        return Err(DbError::Format(
            "a cell pointer leaves the cell content area".to_owned(),
        ));
    }
    Ok(offset)
}

/// Reject a payload above the reader's allocation cap.
fn check_payload_len(payload_len: u64) -> Result<(), DbError> {
    if payload_len > MAX_PAYLOAD_BYTES {
        return Err(DbError::Format(format!(
            "the cell payload is {payload_len} bytes, above the 64 MiB cap"
        )));
    }
    Ok(())
}

/// The number of payload bytes a table leaf cell keeps on its own page.
fn local_payload_len(payload_len: u64, usable: u32) -> u64 {
    let usable = u64::from(usable);
    let max_local = usable - TABLE_PAYLOAD_MARGIN;
    if payload_len <= max_local {
        return payload_len;
    }
    let minimum = ((usable - 12) * 32 / 255) - 23;
    let candidate = minimum + ((payload_len - minimum) % (usable - OVERFLOW_POINTER_BYTES));
    if candidate <= max_local {
        candidate
    } else {
        minimum
    }
}

/// Read a SQLite varint and return its value and byte length.
pub(super) fn read_varint(bytes: &[u8], offset: usize) -> Result<(u64, usize), DbError> {
    let mut value = 0_u64;
    for index in 0..9 {
        let byte = *bytes
            .get(offset + index)
            .ok_or_else(|| DbError::Format("a varint leaves the page".to_owned()))?;
        if index == 8 {
            return Ok(((value << 8) | u64::from(byte), 9));
        }
        value = value
            .checked_mul(128)
            .and_then(|current| current.checked_add(u64::from(byte & 0x7f)))
            .ok_or_else(|| DbError::Format("a varint overflows".to_owned()))?;
        if byte & 0x80 == 0 {
            return Ok((value, index + 1));
        }
    }
    Err(DbError::Format("a varint is too long".to_owned()))
}

/// Read exactly `buffer.len()` bytes, reporting a short file as a format error.
fn read_exact<R: Read>(source: &mut R, buffer: &mut [u8]) -> Result<(), DbError> {
    match source.read_exact(buffer) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::UnexpectedEof => Err(DbError::Format(
            "the embedded database is truncated".to_owned(),
        )),
        Err(error) => Err(DbError::Io(error)),
    }
}

/// Read a big-endian 16-bit value.
fn be_u16(bytes: &[u8], offset: usize) -> Result<u16, DbError> {
    let slice = bytes
        .get(offset..offset + 2)
        .ok_or_else(|| DbError::Format("the page is cut".to_owned()))?;
    Ok(u16::from_be_bytes([slice[0], slice[1]]))
}

/// Read a big-endian 32-bit value.
fn be_u32(bytes: &[u8], offset: usize) -> Result<u32, DbError> {
    let slice = bytes
        .get(offset..offset + 4)
        .ok_or_else(|| DbError::Format("the page is cut".to_owned()))?;
    Ok(u32::from_be_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

#[cfg(test)]
mod tests {
    use super::{cell_offset, check_payload_len, local_payload_len, page_header, read_varint};

    #[test]
    fn a_varint_round_trips_a_small_value() {
        let (value, length) = read_varint(&[0x7f], 0).expect("the varint reads");
        assert_eq!((value, length), (127, 1));
    }

    #[test]
    fn a_varint_reads_a_continuation_byte() {
        let (value, length) = read_varint(&[0x81, 0x00], 0).expect("the varint reads");
        assert_eq!((value, length), (128, 2));
    }

    #[test]
    fn a_cut_varint_is_an_error() {
        assert!(read_varint(&[0x81], 0).is_err());
    }

    #[test]
    fn a_payload_above_the_cap_is_an_error() {
        assert!(check_payload_len(64 * 1024 * 1024).is_ok());
        assert!(check_payload_len(64 * 1024 * 1024 + 1).is_err());
    }

    #[test]
    fn a_page_one_content_area_must_clear_the_header() {
        let mut page = vec![0_u8; 512];
        page[100] = 13; // leaf table page after the 100-byte database header
        page[103..105].copy_from_slice(&1_u16.to_be_bytes());
        page[108..110].copy_from_slice(&100_u16.to_be_bytes());
        // A content start below the pointer array leaves the header.
        page[105..107].copy_from_slice(&104_u16.to_be_bytes());
        assert!(page_header(&page, 1, 512).is_err());
        // A cell pointer inside the header is outside the content area.
        page[105..107].copy_from_slice(&110_u16.to_be_bytes());
        let header = page_header(&page, 1, 512).expect("the header reads");
        assert!(cell_offset(&page, &header, 0).is_err());
    }

    #[test]
    fn the_local_payload_follows_the_page_rules() {
        // A 4096-byte page keeps a small payload and caps a large one.
        assert_eq!(local_payload_len(100, 4096), 100);
        assert_eq!(local_payload_len(4061, 4096), 4061);
        assert_eq!(local_payload_len(6003, 4096), 1911);
        assert_eq!(local_payload_len(4489, 4096), 489);
    }
}
