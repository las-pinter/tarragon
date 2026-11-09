//! ZIP and ZIP-with-prediction layer channels on synthetic files.
//!
//! Each zipped layer must composite to the same pixels as the raw layer that
//! holds the same samples. A cut, garbage, short or oversized stream must fail
//! the render with a typed error and never panic.

mod common;

use std::time::Duration;

use common::{
    DocumentSpec, LayerSpec, build_psd, channel_bytes, composite, composite_bytes, deflate,
    padded_stream,
};
use tarragon_psd::{Deadline, ParseOptions, PsdDocument, PsdError, Viewport};

/// More empty stored blocks than fit one 64 KiB input chunk (5 bytes each).
const BLOCKS_PAST_A_CHUNK: usize = 14_000;

const WIDTH: usize = 6;
const HEIGHT: usize = 5;

/// Samples that wrap in both directions, so the delta code is exercised.
fn pattern(depth: u16, seed: usize) -> Vec<u16> {
    let modulus = if depth == 8 { 256 } else { 65536 };
    (0..WIDTH * HEIGHT)
        .map(|index| {
            u16::try_from((index * 7919 + seed * 104_729 + index * index * 31) % modulus)
                .expect("the sample fits")
        })
        .collect()
}

/// An opaque RGB layer with patterned channels and the given compression code.
fn rgb_layer(depth: u16, code: u16) -> LayerSpec {
    let mut layer = LayerSpec::opaque((0, 0, 5, 6), [0, 0, 0]);
    for (index, channel) in layer.channels.iter_mut().enumerate() {
        channel.1 = pattern(depth, index + 1);
    }
    layer.compression = code;
    layer
}

fn rgb_document(depth: u16, code: u16) -> DocumentSpec {
    let mut spec = DocumentSpec::new(6, 5, depth, 3);
    spec.layers.push(rgb_layer(depth, code));
    spec
}

/// A one-channel gray layer whose channel block is `payload`.
fn gray_document_with_payload(code: u16, payload: Vec<u8>) -> Vec<u8> {
    let mut spec = DocumentSpec::new(6, 5, 8, 1);
    let mut layer = LayerSpec::gray((0, 0, 5, 6), 0);
    layer.compression = code;
    layer.raw_payload = Some(payload);
    spec.layers.push(layer);
    build_psd(&spec)
}

/// A one-layer gray document with the given samples and compression code.
fn gray_document(width: usize, height: usize, depth: u16, code: u16, samples: Vec<u16>) -> Vec<u8> {
    let rows = i32::try_from(height).expect("height");
    let columns = i32::try_from(width).expect("width");
    let mut spec = DocumentSpec::new(
        u32::try_from(width).expect("width"),
        u32::try_from(height).expect("height"),
        depth,
        1,
    );
    let mut layer = LayerSpec::gray((0, 0, rows, columns), 0);
    layer.channels[0].1 = samples;
    layer.compression = code;
    spec.layers.push(layer);
    build_psd(&spec)
}

/// Pseudo-random samples. They do not compress, so the stream is as long as
/// the data.
fn noise(count: usize, depth: u16) -> Vec<u16> {
    let mut state = 0x9E37_79B9u32;
    (0..count)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            let mask = if depth == 8 { 0xFF } else { 0xFFFF };
            u16::try_from(state & mask).expect("the sample fits")
        })
        .collect()
}

/// Composite owned bytes over the full canvas and return the error.
fn composite_error(bytes: Vec<u8>) -> PsdError {
    let document = PsdDocument::from_bytes(bytes, &ParseOptions::none()).expect("the file parses");
    let header = &document.file().header;
    document
        .composite_region(
            Viewport::full(header.width, header.height),
            &ParseOptions::none(),
        )
        .expect_err("the corrupt channel fails the render")
}

#[test]
fn a_16bit_prediction_layer_composites_to_its_pixels() {
    let zipped = composite_bytes(build_psd(&rgb_document(16, 3)));
    let raw = composite_bytes(build_psd(&rgb_document(16, 0)));
    assert_eq!(zipped.skipped_layers(), 0);
    assert_eq!(zipped.rgba8()[3], 255, "the layer is drawn");
    assert_eq!(zipped.rgba8(), raw.rgba8());
}

#[test]
fn zip_layers_match_raw_layers_at_both_depths_and_codes() {
    for depth in [8u16, 16] {
        let raw = composite_bytes(build_psd(&rgb_document(depth, 0)));
        for code in [2u16, 3] {
            let zipped = composite_bytes(build_psd(&rgb_document(depth, code)));
            assert_eq!(zipped.skipped_layers(), 0, "depth {depth} code {code}");
            assert_eq!(zipped.rgba8(), raw.rgba8(), "depth {depth} code {code}");
        }
    }
}

#[test]
fn a_partial_viewport_of_a_zip_layer_matches_the_raw_layer() {
    let viewport = Viewport::new(2, 1, 5, 4);
    for depth in [8u16, 16] {
        let raw = composite(build_psd(&rgb_document(depth, 0)), viewport);
        for code in [2u16, 3] {
            let zipped = composite(build_psd(&rgb_document(depth, code)), viewport);
            assert_eq!(zipped.rgba8(), raw.rgba8(), "depth {depth} code {code}");
        }
    }
}

#[test]
fn a_stream_cut_in_half_is_a_typed_error() {
    for code in [2u16, 3] {
        let full = deflate(&channel_bytes(&pattern(8, 1), WIDTH, 8, code == 3));
        let cut = full[..full.len() / 2].to_vec();
        let error = composite_error(gray_document_with_payload(code, cut));
        assert!(
            matches!(error, PsdError::CorruptChannelData { code: found } if found == code),
            "{error:?}"
        );
    }
}

#[test]
fn a_complete_stream_with_too_few_rows_is_a_typed_error() {
    let short = deflate(&channel_bytes(&pattern(8, 1)[..WIDTH * 2], WIDTH, 8, false));
    let error = composite_error(gray_document_with_payload(2, short));
    assert!(matches!(error, PsdError::CorruptChannelData { code: 2 }));
}

#[test]
fn a_stream_that_inflates_past_the_channel_size_is_a_typed_error() {
    for code in [2u16, 3] {
        let mut bytes = channel_bytes(&pattern(8, 1), WIDTH, 8, code == 3);
        bytes.extend_from_slice(&[7u8; 100]);
        let error = composite_error(gray_document_with_payload(code, deflate(&bytes)));
        assert!(
            matches!(error, PsdError::CorruptChannelData { code: found } if found == code),
            "{error:?}"
        );
    }
}

#[test]
fn a_stream_that_spans_several_input_chunks_matches_the_raw_layer() {
    // 400 x 260 noise is about 104 KB at 8 bit and 208 KB at 16 bit.
    for depth in [8u16, 16] {
        let samples = noise(400 * 260, depth);
        let raw = composite_bytes(gray_document(400, 260, depth, 0, samples.clone()));
        for code in [2u16, 3] {
            let stream = deflate(&channel_bytes(&samples, 400, depth, code == 3));
            assert!(stream.len() > 64 * 1024, "the stream is one chunk or less");
            let zipped = composite_bytes(gray_document(400, 260, depth, code, samples.clone()));
            assert_eq!(zipped.skipped_layers(), 0, "depth {depth} code {code}");
            assert_eq!(zipped.rgba8(), raw.rgba8(), "depth {depth} code {code}");
        }
    }
}

#[test]
fn empty_blocks_across_a_chunk_boundary_do_not_change_the_pixels() {
    for code in [2u16, 3] {
        let bytes = channel_bytes(&pattern(8, 1), WIDTH, 8, code == 3);
        let payload = padded_stream(&bytes, BLOCKS_PAST_A_CHUNK, &[]);
        assert!(payload.len() > 64 * 1024);
        let zipped = composite_bytes(gray_document_with_payload(code, payload));
        let raw = composite_bytes(gray_document(WIDTH, HEIGHT, 8, 0, pattern(8, 1)));
        assert_eq!(zipped.rgba8(), raw.rgba8(), "code {code}");
    }
}

#[test]
fn extra_output_past_a_chunk_boundary_is_a_typed_error() {
    for code in [2u16, 3] {
        let bytes = channel_bytes(&pattern(8, 1), WIDTH, 8, code == 3);
        let payload = padded_stream(&bytes, BLOCKS_PAST_A_CHUNK, &[7u8; 10]);
        let error = composite_error(gray_document_with_payload(code, payload));
        assert!(
            matches!(error, PsdError::CorruptChannelData { code: found } if found == code),
            "{error:?}"
        );
    }
}

#[test]
fn a_stream_without_its_checksum_is_a_typed_error() {
    for code in [2u16, 3] {
        let full = deflate(&channel_bytes(&pattern(8, 1), WIDTH, 8, code == 3));
        let cut = full[..full.len() - 4].to_vec();
        let error = composite_error(gray_document_with_payload(code, cut));
        assert!(
            matches!(error, PsdError::CorruptChannelData { code: found } if found == code),
            "{error:?}"
        );
    }
}

#[test]
fn a_past_deadline_gives_a_timeout_for_a_zip_layer() {
    let bytes = channel_bytes(&pattern(8, 1), WIDTH, 8, false);
    let payload = padded_stream(&bytes, 10, &[]);
    let file = gray_document_with_payload(2, payload);
    let document = PsdDocument::from_bytes(file, &ParseOptions::none()).expect("the file parses");
    let expired = ParseOptions::with_deadline(Deadline::after(Duration::from_secs(0)));
    let result = document.composite_region(Viewport::full(6, 5), &expired);
    assert!(matches!(result, Err(PsdError::Timeout)));
}

#[test]
fn a_high_ratio_stream_decodes_within_the_row_bounds() {
    // 8192 rows of zeros deflate to a few hundred bytes.
    let samples = vec![0u16; 64 * 8192];
    let stream = deflate(&channel_bytes(&samples, 64, 8, false));
    assert!(stream.len() < 4096, "{} bytes", stream.len());
    let zipped = composite_bytes(gray_document(64, 8192, 8, 2, samples.clone()));
    let raw = composite_bytes(gray_document(64, 8192, 8, 0, samples));
    assert_eq!(zipped.rgba8(), raw.rgba8());
}

#[test]
fn a_32bit_zip_layer_is_a_typed_color_mode_error() {
    // The color mode and depth pair is refused when the file opens, so a
    // depth-32 ZIP layer neither panics nor reaches the inflater.
    for code in [2u16, 3] {
        let mut spec = DocumentSpec::new(2, 2, 32, 3);
        let mut layer = LayerSpec::opaque((0, 0, 2, 2), [1, 2, 3]);
        layer.compression = code;
        spec.layers.push(layer);
        let error = PsdDocument::from_bytes(build_psd(&spec), &ParseOptions::none())
            .expect_err("the depth is refused");
        assert!(
            matches!(error, PsdError::UnsupportedColorMode { depth: 32, .. }),
            "{error:?}"
        );
    }
}

#[test]
fn garbage_is_a_typed_error() {
    for code in [2u16, 3] {
        // No zlib header at all.
        let error = composite_error(gray_document_with_payload(code, vec![0xFF; 40]));
        assert!(
            matches!(error, PsdError::CorruptChannelData { .. }),
            "{error:?}"
        );
        // A valid header followed by an invalid deflate block.
        let mut bad_block = vec![0x78, 0x9C];
        bad_block.extend_from_slice(&[0xFF; 40]);
        let error = composite_error(gray_document_with_payload(code, bad_block));
        assert!(
            matches!(error, PsdError::CorruptChannelData { .. }),
            "{error:?}"
        );
    }
}
