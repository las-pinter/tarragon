//! Decode limits on the plain renderer.
//!
//! A header that claims a huge canvas must give a typed error. Without the
//! limits the decoder would try to allocate the whole canvas and abort.

use tarragon_core::error::RenderError;
use tarragon_render::plain::render_plain_image;

/// The CRC-32 that PNG chunks use.
fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = u32::MAX;
    for &byte in bytes {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = if crc & 1 == 1 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

/// Append one PNG chunk with its CRC.
fn push_chunk(png: &mut Vec<u8>, kind: [u8; 4], data: &[u8]) {
    let mut body = kind.to_vec();
    body.extend_from_slice(data);
    png.extend_from_slice(
        &u32::try_from(data.len())
            .expect("small chunk")
            .to_be_bytes(),
    );
    png.extend_from_slice(&body);
    png.extend_from_slice(&crc32(&body).to_be_bytes());
}

/// Build a PNG for an 8-bit RGB canvas that has a header, an empty zlib
/// stream, and an end chunk. The header parses, so the decoder reaches the
/// limit checks before it fails on the missing pixel data.
fn header_only_png(width: u32, height: u32) -> Vec<u8> {
    let mut header = Vec::new();
    header.extend_from_slice(&width.to_be_bytes());
    header.extend_from_slice(&height.to_be_bytes());
    header.extend_from_slice(&[8, 2, 0, 0, 0]);
    // A stored, empty zlib stream with its Adler-32 of 1.
    let zlib = [0x78, 0x01, 0x01, 0, 0, 0xff, 0xff, 0, 0, 0, 1];
    let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
    push_chunk(&mut png, *b"IHDR", &header);
    push_chunk(&mut png, *b"IDAT", &zlib);
    push_chunk(&mut png, *b"IEND", &[]);
    png
}

/// Render a crafted canvas and return the decode error reason.
fn reason_for(tag: &str, width: u32, height: u32) -> String {
    let path = std::env::temp_dir().join(format!(
        "tarragon-render-limits-{tag}-{}.png",
        std::process::id()
    ));
    std::fs::write(&path, header_only_png(width, height)).expect("write the crafted file");
    let result = render_plain_image(&path);
    std::fs::remove_file(&path).ok();
    match result {
        Err(RenderError::Decode { reason, .. }) => reason,
        other => panic!("expected a decode error, got {other:?}"),
    }
}

#[test]
fn a_canvas_far_above_the_byte_cap_is_refused() {
    // 30000 x 30000 x 3 bytes is 2.7 GB and passes the edge cap.
    let reason = reason_for("bytes", 30_000, 30_000);
    assert!(reason.contains("byte cap"), "{reason}");
}

#[test]
fn a_canvas_just_above_the_plain_cap_is_refused() {
    // 20000 x 20000 x 3 bytes is 1.2 GB, above 1 GiB.
    let reason = reason_for("above", 20_000, 20_000);
    assert!(reason.contains("byte cap"), "{reason}");
}

#[test]
fn a_canvas_between_the_document_cap_and_the_plain_cap_passes_the_check() {
    // 14000 x 14000 x 3 bytes is 588 MB: above 512 MiB, below 1 GiB. The
    // byte check lets it through, so the error comes from the missing pixels.
    let reason = reason_for("between", 14_000, 14_000);
    assert!(!reason.contains("byte cap"), "{reason}");
}

#[test]
fn a_canvas_wider_than_the_edge_cap_is_refused() {
    let reason = reason_for("edge", 60_000, 4);
    assert!(
        reason.contains("limit"),
        "expected the dimension limit error, got: {reason}"
    );
}
