//! Deterministic `.kra` fixture builder for the Krita renderer tests.
//!
//! A `.kra` document is a ZIP archive. The builder writes a small archive whose
//! root holds `mergedimage.png` (the flattened canvas) and `preview.png` (a
//! smaller preview), both STORED and both encoded from fixed pixels. It also
//! writes the malformed and compressed variants the tests need. The malformed
//! variants cover a wrong CRC-32, a corrupt deflate stream, a header that
//! declares a canvas above the full-tier edge, and a header that sits exactly
//! on the edge.
//!
//! The committed fixture in `tests/fixtures/kra/` is this builder's output. A
//! check test keeps the two in step. Every archive field is pinned, so a second
//! run writes byte-identical bytes.

#![allow(clippy::cast_possible_truncation)]

use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use image::{DynamicImage, ImageFormat, RgbaImage};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, DateTime, ZipWriter};

/// The flattened canvas member name.
pub const MERGED_MEMBER: &str = "mergedimage.png";
/// The smaller preview member name.
pub const PREVIEW_MEMBER: &str = "preview.png";
/// The merged image width in pixels.
pub const MERGED_WIDTH: u32 = 40;
/// The merged image height in pixels.
pub const MERGED_HEIGHT: u32 = 24;
/// The preview width in pixels.
pub const PREVIEW_WIDTH: u32 = 16;
/// The preview height in pixels.
pub const PREVIEW_HEIGHT: u32 = 12;
/// The oriented image width in pixels.
pub const ORIENTED_WIDTH: u32 = 4;
/// The oriented image height in pixels.
pub const ORIENTED_HEIGHT: u32 = 2;
/// The oriented top-left corner pixel.
pub const ORIENTED_TOP_LEFT: [u8; 4] = [255, 0, 0, 255];
/// The oriented top-right corner pixel.
pub const ORIENTED_TOP_RIGHT: [u8; 4] = [0, 255, 0, 255];
/// The oriented bottom-left corner pixel.
pub const ORIENTED_BOTTOM_LEFT: [u8; 4] = [0, 0, 255, 255];
/// The oriented bottom-right corner pixel.
pub const ORIENTED_BOTTOM_RIGHT: [u8; 4] = [255, 255, 255, 255];
/// An EXIF block with one orientation tag, value 6, little-endian TIFF.
const EXIF_ORIENTATION_6: &[u8] = &[
    0x49, 0x49, 0x2A, 0x00, // TIFF header, little-endian
    0x08, 0x00, 0x00, 0x00, // IFD0 starts at offset 8
    0x01, 0x00, // one entry
    0x12, 0x01, // tag 0x0112, Orientation
    0x03, 0x00, // format 3, SHORT
    0x01, 0x00, 0x00, 0x00, // count 1
    0x06, 0x00, 0x00, 0x00, // value 6, rotate 90 clockwise
    0x00, 0x00, 0x00, 0x00, // no next IFD
];

/// Return the absolute path of a committed fixture.
pub fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/kra")
        .join(name)
}

/// Build the deterministic merged pixels.
pub fn merged_image() -> RgbaImage {
    let mut image = RgbaImage::new(MERGED_WIDTH, MERGED_HEIGHT);
    for (x, y, pixel) in image.enumerate_pixels_mut() {
        let red = ((x * 3 + y * 5) % 256) as u8;
        let green = ((x * 7 + y * 2) % 256) as u8;
        let blue = ((x * 11 + y * 13) % 256) as u8;
        *pixel = image::Rgba([red, green, blue, 255]);
    }
    image
}

/// Build the deterministic preview pixels, smaller and different in color.
pub fn preview_image() -> RgbaImage {
    let mut image = RgbaImage::new(PREVIEW_WIDTH, PREVIEW_HEIGHT);
    for (x, y, pixel) in image.enumerate_pixels_mut() {
        let red = ((x * 17 + y) % 256) as u8;
        let green = ((x + y * 19) % 256) as u8;
        let blue = ((x * 5 + y * 7) % 256) as u8;
        *pixel = image::Rgba([red, green, blue, 128]);
    }
    image
}

/// Encode the merged pixels as a PNG.
pub fn merged_png() -> Vec<u8> {
    png(&DynamicImage::ImageRgba8(merged_image()))
}

/// Encode the preview pixels as a PNG.
pub fn preview_png() -> Vec<u8> {
    png(&DynamicImage::ImageRgba8(preview_image()))
}

/// Build the oriented pixels: four distinct corners on a black field.
pub fn oriented_image() -> RgbaImage {
    let mut image =
        RgbaImage::from_pixel(ORIENTED_WIDTH, ORIENTED_HEIGHT, image::Rgba([0, 0, 0, 255]));
    image.put_pixel(0, 0, image::Rgba(ORIENTED_TOP_LEFT));
    image.put_pixel(ORIENTED_WIDTH - 1, 0, image::Rgba(ORIENTED_TOP_RIGHT));
    image.put_pixel(0, ORIENTED_HEIGHT - 1, image::Rgba(ORIENTED_BOTTOM_LEFT));
    image.put_pixel(
        ORIENTED_WIDTH - 1,
        ORIENTED_HEIGHT - 1,
        image::Rgba(ORIENTED_BOTTOM_RIGHT),
    );
    image
}

/// Encode the oriented pixels as a PNG with an orientation 6 EXIF chunk.
pub fn oriented_png() -> Vec<u8> {
    with_exif_orientation(&png(&DynamicImage::ImageRgba8(oriented_image())))
}

/// Build the committed fixture: a merged image and a smaller preview.
pub fn build_default() -> Vec<u8> {
    build_archive(&[
        (MERGED_MEMBER, &merged_png()),
        (PREVIEW_MEMBER, &preview_png()),
    ])
}

/// Build an archive that holds only `preview.png`.
pub fn build_preview_only() -> Vec<u8> {
    build_archive(&[(PREVIEW_MEMBER, &preview_png())])
}

/// Build an archive whose members use deflate compression.
pub fn build_deflated() -> Vec<u8> {
    build_archive_with(
        CompressionMethod::Deflated,
        &[
            (MERGED_MEMBER, &merged_png()),
            (PREVIEW_MEMBER, &preview_png()),
        ],
    )
}

/// Build an archive with a deflated member that declares a wrong CRC-32.
pub fn build_corrupt_crc() -> Vec<u8> {
    let mut bytes = build_archive_with(
        CompressionMethod::Deflated,
        &[(MERGED_MEMBER, &merged_png())],
    );
    patch_crc(&mut bytes, 0);
    bytes
}

/// Build an archive with a deflated member whose CRC is correct.
///
/// The first eight bytes of the compressed stream become `0xFF`, which is not
/// a valid deflate block header, so the decompressor fails.
pub fn build_corrupt_deflate_stream() -> Vec<u8> {
    let mut bytes = build_archive_with(
        CompressionMethod::Deflated,
        &[(MERGED_MEMBER, &merged_png())],
    );
    corrupt_first_member_data(&mut bytes);
    bytes
}

/// Build an archive whose member declares a canvas larger than the full tier.
pub fn build_oversized_canvas() -> Vec<u8> {
    build_archive(&[(MERGED_MEMBER, &oversized_canvas_png())])
}

/// Build an archive whose member declares a canvas on the full-tier edge.
pub fn build_boundary_canvas() -> Vec<u8> {
    build_archive(&[(MERGED_MEMBER, &boundary_canvas_png())])
}

/// Build an archive whose only image member carries orientation 6.
pub fn build_oriented() -> Vec<u8> {
    build_archive(&[(MERGED_MEMBER, &oriented_png())])
}

/// Build an empty file: no archive at all.
pub fn build_empty() -> Vec<u8> {
    Vec::new()
}

/// Build an archive whose image member holds bytes that are not a PNG.
pub fn build_garbage_member() -> Vec<u8> {
    build_archive(&[(MERGED_MEMBER, b"this member holds no png")])
}

/// Build an archive with neither image member.
pub fn build_without_image() -> Vec<u8> {
    build_archive(&[("layers/layer0", b"not an image")])
}

/// Build an archive whose only image member is nested under a folder.
pub fn build_nested() -> Vec<u8> {
    build_archive(&[("sub/mergedimage.png", &merged_png())])
}

/// Build an archive whose only image member has an upper-case name.
pub fn build_uppercase() -> Vec<u8> {
    build_archive(&[("MERGEDIMAGE.PNG", &merged_png())])
}

/// Build bytes that are not a ZIP archive.
pub fn build_garbage() -> Vec<u8> {
    b"no zip archive lives here".to_vec()
}

/// Build the committed fixture and cut it short.
pub fn build_truncated() -> Vec<u8> {
    let mut bytes = build_default();
    bytes.truncate(bytes.len() / 2);
    bytes
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
            std::env::temp_dir().join(format!("tarragon-kra-{tag}-{}-{id}", std::process::id()));
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

/// Encode one image as a PNG.
fn png(image: &DynamicImage) -> Vec<u8> {
    let mut bytes = Vec::new();
    image
        .write_to(&mut Cursor::new(&mut bytes), ImageFormat::Png)
        .expect("the png encodes");
    bytes
}

/// Build a PNG that declares an oversized 8-bit RGBA canvas in its header.
///
/// The data stream is empty, so only the header can reject this member.
pub fn oversized_canvas_png() -> Vec<u8> {
    /// One pixel above the full-tier 8192 edge on both sides.
    const OVERSIZED_EDGE: u32 = 9000;
    header_only_png(OVERSIZED_EDGE)
}

/// Build a PNG that declares an 8-bit RGBA canvas on the 8192 pixel edge.
///
/// The header is the only content, so the edge gate alone decides the outcome.
pub fn boundary_canvas_png() -> Vec<u8> {
    /// The full-tier edge, taken exactly.
    const BOUNDARY_EDGE: u32 = 8192;
    header_only_png(BOUNDARY_EDGE)
}

/// Build a PNG with an 8-bit RGBA header of the given square edge and no data.
fn header_only_png(edge: u32) -> Vec<u8> {
    let mut header = Vec::new();
    header.extend_from_slice(&edge.to_be_bytes());
    header.extend_from_slice(&edge.to_be_bytes());
    header.extend_from_slice(&[8, 6, 0, 0, 0]);
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]);
    push_png_chunk(&mut bytes, *b"IHDR", &header);
    push_png_chunk(&mut bytes, *b"IEND", &[]);
    bytes
}

/// Append one PNG chunk with its length and CRC.
fn push_png_chunk(out: &mut Vec<u8>, kind: [u8; 4], data: &[u8]) {
    let length = u32::try_from(data.len()).expect("the chunk is short");
    let mut body = Vec::with_capacity(4 + data.len());
    body.extend_from_slice(&kind);
    body.extend_from_slice(data);
    out.extend_from_slice(&length.to_be_bytes());
    out.extend_from_slice(&body);
    out.extend_from_slice(&crc32(&body).to_be_bytes());
}

/// Insert an orientation 6 `eXIf` chunk after the PNG header chunk.
pub fn with_exif_orientation(png: &[u8]) -> Vec<u8> {
    const SIGNATURE_LEN: usize = 8;
    const CHUNK_HEADER_LEN: usize = 8;
    const CRC_LEN: usize = 4;
    let header_len = u32::from_be_bytes(
        png[SIGNATURE_LEN..SIGNATURE_LEN + 4]
            .try_into()
            .expect("four length bytes"),
    );
    let insert_at = SIGNATURE_LEN
        + CHUNK_HEADER_LEN
        + usize::try_from(header_len).expect("the header fits")
        + CRC_LEN;
    let mut chunk = Vec::new();
    chunk.extend_from_slice(
        &u32::try_from(EXIF_ORIENTATION_6.len())
            .expect("the block is short")
            .to_be_bytes(),
    );
    chunk.extend_from_slice(b"eXIf");
    chunk.extend_from_slice(EXIF_ORIENTATION_6);
    chunk.extend_from_slice(&crc32(&chunk[4..]).to_be_bytes());
    let mut out = Vec::with_capacity(png.len() + chunk.len());
    out.extend_from_slice(&png[..insert_at]);
    out.extend_from_slice(&chunk);
    out.extend_from_slice(&png[insert_at..]);
    out
}

/// Compute the CRC-32 that a PNG chunk carries.
fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFF_u32;
    for &byte in bytes {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            let mask = 0_u32.wrapping_sub(crc & 1);
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}

/// Overwrite the CRC-32 of the first member, in both headers.
fn patch_crc(bytes: &mut [u8], crc: u32) {
    const LOCAL_CRC_OFFSET: usize = 14;
    const CENTRAL_CRC_OFFSET: usize = 16;
    if let Some(start) = find_signature(bytes, b"PK\x03\x04") {
        bytes[start + LOCAL_CRC_OFFSET..start + LOCAL_CRC_OFFSET + 4]
            .copy_from_slice(&crc.to_le_bytes());
    }
    if let Some(start) = find_signature(bytes, b"PK\x01\x02") {
        bytes[start + CENTRAL_CRC_OFFSET..start + CENTRAL_CRC_OFFSET + 4]
            .copy_from_slice(&crc.to_le_bytes());
    }
}

/// Return the first position of `signature` in `bytes`.
fn find_signature(bytes: &[u8], signature: &[u8]) -> Option<usize> {
    bytes
        .windows(signature.len())
        .position(|window| window == signature)
}

/// Overwrite the first eight data bytes of the first local member with `0xFF`.
fn corrupt_first_member_data(bytes: &mut [u8]) {
    const NAME_LEN_OFFSET: usize = 26;
    const EXTRA_LEN_OFFSET: usize = 28;
    const LOCAL_HEADER_LEN: usize = 30;
    let Some(start) = find_signature(bytes, b"PK\x03\x04") else {
        return;
    };
    let name_len = usize::from(u16::from_le_bytes([
        bytes[start + NAME_LEN_OFFSET],
        bytes[start + NAME_LEN_OFFSET + 1],
    ]));
    let extra_len = usize::from(u16::from_le_bytes([
        bytes[start + EXTRA_LEN_OFFSET],
        bytes[start + EXTRA_LEN_OFFSET + 1],
    ]));
    let data_start = start + LOCAL_HEADER_LEN + name_len + extra_len;
    if let Some(data) = bytes.get_mut(data_start..data_start + 8) {
        data.fill(0xFF);
    }
}

/// Build a STORED ZIP archive from the given members.
fn build_archive(entries: &[(&str, &[u8])]) -> Vec<u8> {
    build_archive_with(CompressionMethod::Stored, entries)
}

/// Build a ZIP archive with the given compression method.
fn build_archive_with(method: CompressionMethod, entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default()
        .compression_method(method)
        .last_modified_time(DateTime::default())
        .unix_permissions(0o644);
    for (name, bytes) in entries {
        writer
            .start_file(*name, options)
            .expect("the member starts");
        writer
            .write_all(bytes)
            .expect("the member bytes are written");
    }
    writer.finish().expect("the archive finishes").into_inner()
}
