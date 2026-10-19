//! Renderer for Krita `.kra` documents.
//!
//! A `.kra` document is a ZIP archive. Krita stores the flattened canvas as
//! `mergedimage.png` at the archive root and a smaller `preview.png` beside it.
//! The renderer opens the archive, reads the first member that exists in that
//! order, and decodes it with the EXIF orientation applied. Only exact
//! root-level names match: a nested or differently cased name does not.
//!
//! A stored member streams from the archive into the decoder, so the payload
//! never sits in memory as a whole. A compressed member spools under a byte
//! cap, because the ZIP reader offers no seekable handle for it. The seekable
//! ZIP handle skips the CRC check on a stored member, so stored bytes reach the
//! PNG decoder unchecked.
//!
//! Every decode is bounded. The 8192 pixel edge is the enforced bound: the PNG
//! decoder rejects a canvas wider or taller than it. The decoder byte check is
//! unreachable defence in depth today, and it guards the cap if the edge rises.
//!
//! A document with neither member, or with member bytes that do not decode,
//! takes the typed error path: the caller shows the error card. The renderer
//! never resizes; the cache store owns the tier scaling.

use std::io::{self, BufRead, BufReader, Cursor, Read, Seek};
use std::path::Path;

use image::{DynamicImage, ImageDecoder, ImageFormat, ImageReader, Limits};
use tarragon_core::error::RenderError;
use zip::ZipArchive;
use zip::result::ZipError;

use crate::renderer::RenderedImage;

/// The flattened canvas member. It wins when both members exist.
const MERGED_MEMBER: &str = "mergedimage.png";
/// The smaller preview member. It is the fallback.
const PREVIEW_MEMBER: &str = "preview.png";
/// The largest member payload the renderer spools, in bytes.
///
/// A 16-bit RGBA canvas at the 8192 pixel edge decodes to 512 MiB. A stored
/// PNG member is larger than that decoded buffer by one filter byte per row
/// plus chunk framing, so the member cap sits above the decoder cap.
const MAX_MEMBER_BYTES: u64 = 600 * 1024 * 1024;
/// The decoder allocation cap, in bytes.
///
/// An 8-bit RGBA canvas at the 8192 edge needs 256 MiB. A 16-bit RGBA canvas
/// at the same edge needs 512 MiB, which equals the cap exactly. The strict
/// greater-than check lets that canvas pass.
const MAX_DECODE_ALLOC_BYTES: u64 = 512 * 1024 * 1024;
/// The largest accepted canvas edge, in pixels.
///
/// This is the full-tier edge. The PNG decoder checks it at the header.
const MAX_IMAGE_EDGE: u32 = 8192;

/// Render a `.kra` file to a `DynamicImage`.
///
/// The renderer reads `mergedimage.png` when the archive holds it and falls
/// back to `preview.png` otherwise. The member is the full resolution the
/// document stores; the cache store owns the tier scaling.
///
/// # Errors
///
/// Returns [`RenderError::Io`] when the file cannot be opened or read and the
/// read error kind is not a corruption kind. Returns [`RenderError::Decode`]
/// when the archive structure or a member payload is corrupt, when neither
/// member exists, or when the member PNG does not decode.
///
/// The read error kind decides the variant. A read with kind `InvalidData`
/// (a CRC mismatch), `InvalidInput` (a corrupt deflate stream), or
/// `UnexpectedEof` (member data shorter than the declared size) means corrupt
/// bytes, so it maps to `Decode`. Every other kind maps to `Io`.
pub fn render_kra_image(path: &Path) -> Result<RenderedImage, RenderError> {
    let file = std::fs::File::open(path).map_err(|source| RenderError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let mut archive = ZipArchive::new(file).map_err(|source| map_zip_error(path, source))?;
    for name in [MERGED_MEMBER, PREVIEW_MEMBER] {
        match archive.by_name_seek(name) {
            Ok(member) => return decode_png(path, BufReader::new(member)),
            Err(ZipError::FileNotFound) => {}
            // A compressed member has no seekable handle, so spool it.
            Err(ZipError::UnsupportedArchive(_)) => {
                return render_compressed_member(&mut archive, name, path);
            }
            Err(other) => return Err(map_zip_error(path, other)),
        }
    }
    Err(decode_error(
        path,
        format!("the archive holds no {MERGED_MEMBER} or {PREVIEW_MEMBER} member"),
    ))
}

/// Read a compressed member under the byte cap and decode it.
fn render_compressed_member<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    name: &str,
    path: &Path,
) -> Result<RenderedImage, RenderError> {
    let mut member = archive
        .by_name(name)
        .map_err(|source| map_zip_error(path, source))?;
    if member.size() > MAX_MEMBER_BYTES {
        return Err(member_too_large(path, name));
    }
    // The declared size can lie, so cap the actual read too.
    let mut bytes = Vec::new();
    member
        .by_ref()
        .take(MAX_MEMBER_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|source| map_read_error(path, source))?;
    if bytes.len() as u64 > MAX_MEMBER_BYTES {
        return Err(member_too_large(path, name));
    }
    decode_png(path, BufReader::new(Cursor::new(bytes)))
}

/// Build the error for a member that exceeds the byte cap.
fn member_too_large(path: &Path, name: &str) -> RenderError {
    decode_error(
        path,
        format!("the {name} member exceeds the {MAX_MEMBER_BYTES} byte cap"),
    )
}

/// Decode the member PNG and apply the EXIF orientation.
fn decode_png<R: BufRead + Seek>(path: &Path, reader: R) -> Result<RenderedImage, RenderError> {
    let mut reader = ImageReader::with_format(reader, ImageFormat::Png);
    let mut limits = Limits::default();
    // Assigning the documented default pins it against an upstream change.
    limits.max_alloc = Some(MAX_DECODE_ALLOC_BYTES);
    limits.max_image_width = Some(MAX_IMAGE_EDGE);
    limits.max_image_height = Some(MAX_IMAGE_EDGE);
    reader.limits(limits);
    let mut decoder = reader
        .into_decoder()
        .map_err(|source| RenderError::Decode {
            path: path.to_path_buf(),
            reason: source.to_string(),
        })?;
    // `from_decoder` allocates without a limit check, so check the size here.
    let total_bytes = decoder.total_bytes();
    if total_bytes > MAX_DECODE_ALLOC_BYTES {
        return Err(decode_error(
            path,
            format!(
                "the canvas needs {total_bytes} bytes, above the {MAX_DECODE_ALLOC_BYTES} byte cap"
            ),
        ));
    }
    // `decode()` does not apply orientation, so read the tag first.
    let orientation = decoder
        .orientation()
        .map_err(|source| RenderError::Decode {
            path: path.to_path_buf(),
            reason: source.to_string(),
        })?;
    let mut image = DynamicImage::from_decoder(decoder).map_err(|source| RenderError::Decode {
        path: path.to_path_buf(),
        reason: source.to_string(),
    })?;
    image.apply_orientation(orientation);
    Ok(image)
}

/// Build a decode error with a reason.
fn decode_error(path: &Path, reason: String) -> RenderError {
    RenderError::Decode {
        path: path.to_path_buf(),
        reason,
    }
}

/// Map a read error onto the render error type by its kind.
fn map_read_error(path: &Path, source: io::Error) -> RenderError {
    match source.kind() {
        io::ErrorKind::InvalidData | io::ErrorKind::InvalidInput | io::ErrorKind::UnexpectedEof => {
            RenderError::Decode {
                path: path.to_path_buf(),
                reason: source.to_string(),
            }
        }
        _ => RenderError::Io {
            path: path.to_path_buf(),
            source,
        },
    }
}

/// Map a ZIP reader error onto the render error type.
fn map_zip_error(path: &Path, error: ZipError) -> RenderError {
    match error {
        ZipError::Io(source) => map_read_error(path, source),
        other => decode_error(path, other.to_string()),
    }
}
