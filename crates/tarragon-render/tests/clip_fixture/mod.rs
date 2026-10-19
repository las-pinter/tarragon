//! Deterministic `.clip` fixture builder for the CLIP renderer tests.
//!
//! The builder writes a valid SQLite database by hand. Page 1 holds the schema
//! row, page 2 holds one `CanvasPreview` row, and overflow pages carry the PNG
//! preview when it does not fit on the row page. The builder also writes the
//! binary prefix and an optional trailing payload.
//!
//! The committed fixture in `tests/fixtures/clip/` is this builder's output. A
//! check test keeps the two in step.

#![allow(clippy::cast_possible_truncation)]

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use image::{DynamicImage, ImageFormat, RgbaImage};

/// The table that holds the preview row.
pub const PREVIEW_TABLE: &str = "CanvasPreview";
/// The column that holds the PNG blob.
pub const PREVIEW_COLUMN: &str = "ImageData";
/// The preview width in pixels.
pub const PREVIEW_WIDTH: u32 = 48;
/// The preview height in pixels.
pub const PREVIEW_HEIGHT: u32 = 32;

/// The SQLite page size the fixture uses.
const PAGE_SIZE: usize = 4096;
/// The offset of the SQLite header inside the `.clip` file.
const HEADER_OFFSET: usize = 136;
/// The binary prefix the fixture writes before the database.
const PREFIX: &[u8] = b"CSFCHUNK";
/// The byte fill of the optional trailing payload.
const TRAILING_FILL: u8 = 0x5a;

/// Return the absolute path of a committed fixture.
pub fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/clip")
        .join(name)
}

/// Build the deterministic preview pixels.
pub fn preview_image() -> RgbaImage {
    let mut image = RgbaImage::new(PREVIEW_WIDTH, PREVIEW_HEIGHT);
    for (x, y, pixel) in image.enumerate_pixels_mut() {
        let red = ((x * 5 + y * 3) % 256) as u8;
        let green = ((x * 7 + y * 11) % 256) as u8;
        let blue = ((x * 13 + y * 17) % 256) as u8;
        // A fifth of the pixels stay partly transparent, so the alpha survives
        // the round trip.
        let alpha = if (x + y) % 5 == 0 { 128 } else { 255 };
        *pixel = image::Rgba([red, green, blue, alpha]);
    }
    image
}

/// Encode the deterministic preview pixels as a PNG.
pub fn preview_png() -> Vec<u8> {
    let mut bytes = Vec::new();
    DynamicImage::ImageRgba8(preview_image())
        .write_to(&mut std::io::Cursor::new(&mut bytes), ImageFormat::Png)
        .expect("the preview png encodes");
    bytes
}

/// Build the committed fixture: one preview with overflow pages, no junk.
pub fn build_default() -> Vec<u8> {
    build_clip(&preview_png(), 0, 0)
}

/// Build a `.clip` file around one PNG blob.
///
/// `prefix_junk` adds zeros between the binary prefix and the SQLite header.
/// `trailing_junk` appends bytes after the last database page.
pub fn build_clip(png: &[u8], prefix_junk: usize, trailing_junk: usize) -> Vec<u8> {
    assemble(
        PREVIEW_TABLE,
        PREVIEW_COLUMN,
        &[Field::Blob(png)],
        prefix_junk,
        trailing_junk,
    )
}

/// Build a `.clip` file whose database has a table with a different name.
pub fn build_with_table(table: &str, png: &[u8]) -> Vec<u8> {
    assemble(table, PREVIEW_COLUMN, &[Field::Blob(png)], 0, 0)
}

/// Build a `.clip` file whose preview table has a different column name.
pub fn build_with_column(column: &str, png: &[u8]) -> Vec<u8> {
    assemble(PREVIEW_TABLE, column, &[Field::Blob(png)], 0, 0)
}

/// Build a `.clip` file whose preview row holds a NULL `ImageData` value.
pub fn build_with_null() -> Vec<u8> {
    assemble(PREVIEW_TABLE, PREVIEW_COLUMN, &[Field::Null], 0, 0)
}

/// The number of pages a two-level tree uses before its overflow pages.
const TWO_LEVEL_BASE_PAGES: usize = 6;

/// Build a `.clip` file with two-level schema and preview b-trees.
///
/// Page 1 is an interior schema root with one cell to page 2 and a right-most
/// child on page 3. Page 4 is an interior preview root with one cell to page 5
/// and a right-most child on page 6. Overflow pages follow page 6.
pub fn build_two_level(png: &[u8]) -> Vec<u8> {
    let create_sql = format!("CREATE TABLE {PREVIEW_TABLE} ({PREVIEW_COLUMN} BLOB)");
    let schema_payload = schema_record(PREVIEW_TABLE, &create_sql, 4);
    let filler_schema = schema_record("Zz", "CREATE TABLE Zz (x BLOB)", 6);
    let row_payload = record(&[Field::Blob(png)]);
    let filler_payload = record(&[Field::Blob(b"filler")]);
    let local = local_payload_len(row_payload.len());
    let overflow_bytes = row_payload.len() - local;
    let page_count = TWO_LEVEL_BASE_PAGES + overflow_bytes.div_ceil(PAGE_SIZE - 4);

    let mut page_one = vec![0_u8; PAGE_SIZE];
    write_database_header(&mut page_one, page_count as u32);
    let schema_cell = interior_cell(2, 2);
    let schema_at = PAGE_SIZE - schema_cell.len();
    page_one[schema_at..].copy_from_slice(&schema_cell);
    write_interior_header(&mut page_one, 100, schema_at, 3);

    let mut page_two = vec![0_u8; PAGE_SIZE];
    let cell = leaf_cell(1, &schema_payload);
    let at = PAGE_SIZE - cell.len();
    page_two[at..].copy_from_slice(&cell);
    write_leaf_header(&mut page_two, 0, at);

    let mut page_three = vec![0_u8; PAGE_SIZE];
    let cell = leaf_cell(3, &filler_schema);
    let at = PAGE_SIZE - cell.len();
    page_three[at..].copy_from_slice(&cell);
    write_leaf_header(&mut page_three, 0, at);

    let mut page_four = vec![0_u8; PAGE_SIZE];
    let cell = interior_cell(5, 2);
    let at = PAGE_SIZE - cell.len();
    page_four[at..].copy_from_slice(&cell);
    write_interior_header(&mut page_four, 0, at, 6);

    let mut page_five = vec![0_u8; PAGE_SIZE];
    let first_overflow = TWO_LEVEL_BASE_PAGES as u32 + 1;
    let cell = row_cell(&row_payload, local, first_overflow);
    let at = PAGE_SIZE - cell.len();
    page_five[at..].copy_from_slice(&cell);
    write_leaf_header(&mut page_five, 0, at);

    let mut page_six = vec![0_u8; PAGE_SIZE];
    let cell = leaf_cell(3, &filler_payload);
    let at = PAGE_SIZE - cell.len();
    page_six[at..].copy_from_slice(&cell);
    write_leaf_header(&mut page_six, 0, at);

    let mut pages = vec![
        page_one, page_two, page_three, page_four, page_five, page_six,
    ];
    pages.extend(overflow_pages(&row_payload, local, first_overflow));
    assert_eq!(pages.len(), page_count, "the page count is exact");
    wrap(&pages, 0, 0)
}

/// The number of overflow pages a payload of `payload_len` bytes needs.
pub fn overflow_pages_for(payload_len: usize) -> usize {
    let local = local_payload_len(payload_len);
    (payload_len - local).div_ceil(PAGE_SIZE - 4)
}

/// A temporary folder that removes itself on drop.
pub struct TempDir {
    path: PathBuf,
}

impl TempDir {
    /// Create a unique temp folder with the given tag.
    pub fn new(tag: &str) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("tarragon-clip-{tag}-{}-{id}", std::process::id()));
        std::fs::create_dir_all(&path).expect("the temp folder is created");
        Self { path }
    }

    /// Return the folder path.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// One record field the fixture writes.
enum Field<'a> {
    /// The SQL NULL value.
    Null,
    /// A text value.
    Text(&'a [u8]),
    /// An integer value.
    Integer(i64),
    /// A blob value.
    Blob(&'a [u8]),
}

/// Build the whole `.clip` file from one table name and one PNG blob.
fn assemble(
    table: &str,
    column: &str,
    fields: &[Field<'_>],
    prefix_junk: usize,
    trailing_junk: usize,
) -> Vec<u8> {
    let create_sql = format!("CREATE TABLE {table} ({column} BLOB)");
    let schema_payload = schema_record(table, &create_sql, 2);
    let row_payload = record(fields);
    let schema_cell = leaf_cell(1, &schema_payload);
    let local = local_payload_len(row_payload.len());
    let overflow_bytes = row_payload.len() - local;
    let overflow_count = overflow_bytes.div_ceil(PAGE_SIZE - 4);
    let page_count = 2 + overflow_count;

    let mut page_one = vec![0_u8; PAGE_SIZE];
    write_database_header(&mut page_one, page_count as u32);
    let schema_at = PAGE_SIZE - schema_cell.len();
    page_one[schema_at..].copy_from_slice(&schema_cell);
    write_leaf_header(&mut page_one, 100, schema_at);

    let mut page_two = vec![0_u8; PAGE_SIZE];
    let cell = row_cell(&row_payload, local, 3);
    let row_at = PAGE_SIZE - cell.len();
    page_two[row_at..].copy_from_slice(&cell);
    write_leaf_header(&mut page_two, 0, row_at);

    let mut pages = vec![page_one, page_two];
    pages.extend(overflow_pages(&row_payload, local, 3));
    assert_eq!(pages.len(), page_count, "the page count is exact");
    wrap(&pages, prefix_junk, trailing_junk)
}

/// Assemble the binary prefix, the pages, and the trailing payload.
fn wrap(pages: &[Vec<u8>], prefix_junk: usize, trailing_junk: usize) -> Vec<u8> {
    let mut file = Vec::with_capacity(HEADER_OFFSET + prefix_junk + pages.len() * PAGE_SIZE);
    file.extend_from_slice(PREFIX);
    file.resize(HEADER_OFFSET + prefix_junk, 0);
    for page in pages {
        file.extend_from_slice(page);
    }
    file.resize(file.len() + trailing_junk, TRAILING_FILL);
    file
}

/// Build one table leaf cell and its overflow pointer.
fn row_cell(payload: &[u8], local: usize, first_overflow: u32) -> Vec<u8> {
    let mut cell = Vec::with_capacity(local + 12);
    push_varint(&mut cell, payload.len() as u64);
    push_varint(&mut cell, 1);
    cell.extend_from_slice(&payload[..local]);
    if payload.len() > local {
        cell.extend_from_slice(&first_overflow.to_be_bytes());
    }
    cell
}

/// Build the overflow pages that continue one cell payload.
fn overflow_pages(payload: &[u8], local: usize, first: u32) -> Vec<Vec<u8>> {
    let mut pages = Vec::new();
    let mut rest = &payload[local..];
    let mut number = first;
    while !rest.is_empty() {
        let take = rest.len().min(PAGE_SIZE - 4);
        let mut page = vec![0_u8; PAGE_SIZE];
        let next = if rest.len() > take { number + 1 } else { 0 };
        page[..4].copy_from_slice(&next.to_be_bytes());
        page[4..4 + take].copy_from_slice(&rest[..take]);
        pages.push(page);
        rest = &rest[take..];
        number += 1;
    }
    pages
}

/// Build one `sqlite_master` row.
fn schema_record(table: &str, create_sql: &str, root_page: i64) -> Vec<u8> {
    record(&[
        Field::Text(b"table"),
        Field::Text(table.as_bytes()),
        Field::Text(table.as_bytes()),
        Field::Integer(root_page),
        Field::Text(create_sql.as_bytes()),
    ])
}

/// Write the database header of page 1.
fn write_database_header(page: &mut [u8], page_count: u32) {
    page[..16].copy_from_slice(b"SQLite format 3\0");
    page[16..18].copy_from_slice(&(PAGE_SIZE as u16).to_be_bytes());
    page[18] = 1; // write version
    page[19] = 1; // read version
    page[20] = 0; // reserved bytes per page
    page[21] = 64; // maximum embedded payload fraction
    page[22] = 32; // minimum embedded payload fraction
    page[23] = 32; // leaf payload fraction
    page[24..28].copy_from_slice(&1_u32.to_be_bytes()); // file change counter
    page[28..32].copy_from_slice(&page_count.to_be_bytes());
    page[40..44].copy_from_slice(&1_u32.to_be_bytes()); // schema cookie
    page[44..48].copy_from_slice(&4_u32.to_be_bytes()); // schema format
    page[56..60].copy_from_slice(&1_u32.to_be_bytes()); // text encoding, UTF-8
    page[92..96].copy_from_slice(&1_u32.to_be_bytes()); // version valid for
    page[96..100].copy_from_slice(&3_045_000_u32.to_be_bytes()); // SQLite version
}

/// Write a leaf table page header and its one cell pointer.
fn write_leaf_header(page: &mut [u8], offset: usize, cell_at: usize) {
    page[offset] = 13; // leaf table page
    page[offset + 3..offset + 5].copy_from_slice(&1_u16.to_be_bytes());
    page[offset + 5..offset + 7].copy_from_slice(&(cell_at as u16).to_be_bytes());
    page[offset + 8..offset + 10].copy_from_slice(&(cell_at as u16).to_be_bytes());
}

/// Write an interior table page header and its one cell pointer.
fn write_interior_header(page: &mut [u8], offset: usize, cell_at: usize, right_most: u32) {
    page[offset] = 5; // interior table page
    page[offset + 3..offset + 5].copy_from_slice(&1_u16.to_be_bytes());
    page[offset + 5..offset + 7].copy_from_slice(&(cell_at as u16).to_be_bytes());
    page[offset + 8..offset + 12].copy_from_slice(&right_most.to_be_bytes());
    page[offset + 12..offset + 14].copy_from_slice(&(cell_at as u16).to_be_bytes());
}

/// Build one interior table cell with no payload.
fn interior_cell(child: u32, key: u64) -> Vec<u8> {
    let mut cell = Vec::with_capacity(5);
    cell.extend_from_slice(&child.to_be_bytes());
    push_varint(&mut cell, key);
    cell
}

/// Build one table leaf cell with no overflow.
fn leaf_cell(rowid: u64, payload: &[u8]) -> Vec<u8> {
    let mut cell = Vec::with_capacity(payload.len() + 12);
    push_varint(&mut cell, payload.len() as u64);
    push_varint(&mut cell, rowid);
    cell.extend_from_slice(payload);
    cell
}

/// Build one record payload from its fields.
fn record(fields: &[Field<'_>]) -> Vec<u8> {
    let mut serials = Vec::new();
    let mut body = Vec::new();
    for field in fields {
        let (serial, bytes) = encode(field);
        push_varint(&mut serials, serial);
        body.extend_from_slice(&bytes);
    }
    let header_len = 1 + serials.len();
    assert!(header_len < 128, "the fixture record header is one byte");
    let mut record = Vec::with_capacity(header_len + body.len());
    record.push(header_len as u8);
    record.extend_from_slice(&serials);
    record.extend_from_slice(&body);
    record
}

/// Encode one record field as a serial type and body bytes.
fn encode(field: &Field<'_>) -> (u64, Vec<u8>) {
    match field {
        Field::Null => (0, Vec::new()),
        Field::Text(text) => (13 + 2 * text.len() as u64, text.to_vec()),
        Field::Blob(blob) => (12 + 2 * blob.len() as u64, blob.to_vec()),
        Field::Integer(value) => {
            let byte = u8::try_from(*value).expect("the fixture integer fits one byte");
            (1, vec![byte])
        }
    }
}

/// The number of payload bytes a table leaf cell keeps on its own page.
fn local_payload_len(payload_len: usize) -> usize {
    let usable = PAGE_SIZE as u64;
    let max_local = usable - 35;
    let payload = payload_len as u64;
    if payload <= max_local {
        return payload_len;
    }
    let minimum = ((usable - 12) * 32 / 255) - 23;
    let candidate = minimum + ((payload - minimum) % (usable - 4));
    if candidate <= max_local {
        candidate as usize
    } else {
        minimum as usize
    }
}

/// Write a SQLite varint.
fn push_varint(out: &mut Vec<u8>, value: u64) {
    let mut groups = [0_u8; 9];
    let mut count = 0;
    let mut remaining = value;
    loop {
        groups[count] = (remaining & 0x7f) as u8;
        count += 1;
        remaining >>= 7;
        if remaining == 0 {
            break;
        }
    }
    for index in (0..count).rev() {
        let last = index == 0;
        out.push(if last {
            groups[index]
        } else {
            groups[index] | 0x80
        });
    }
}
