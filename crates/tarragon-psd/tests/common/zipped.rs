//! zlib channel payloads for the synthetic builder.

use std::io::Write;

use flate2::Compression;
use flate2::write::ZlibEncoder;

use super::builder::{LayerSpec, push_samples, sample_bytes};

/// Compress bytes into one zlib stream.
pub fn deflate(bytes: &[u8]) -> Vec<u8> {
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder
        .write_all(bytes)
        .expect("the encoder takes the bytes");
    encoder.finish().expect("the encoder finishes")
}

/// One zlib stream of `bytes`, then `empty_blocks` empty stored blocks, then
/// `extra`. The empty blocks give no output, so they push `extra` far into the
/// compressed data.
pub fn padded_stream(bytes: &[u8], empty_blocks: usize, extra: &[u8]) -> Vec<u8> {
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder
        .write_all(bytes)
        .expect("the encoder takes the bytes");
    // A sync flush ends on a byte boundary, so whole blocks can follow it.
    encoder.flush().expect("the encoder flushes");
    for _ in 0..empty_blocks {
        encoder.get_mut().extend_from_slice(&[0, 0, 0, 0xFF, 0xFF]);
    }
    encoder
        .write_all(extra)
        .expect("the encoder takes the extra");
    encoder.finish().expect("the encoder finishes")
}

/// The raw bytes of a channel, row by row, in the file byte order.
///
/// With `predicted`, each sample after the first in a row is stored as its
/// difference from the sample before it, modulo the sample size.
pub fn channel_bytes(samples: &[u16], width: usize, depth: u16, predicted: bool) -> Vec<u8> {
    let mut out = Vec::new();
    if width == 0 {
        return out;
    }
    let modulus = if depth == 8 { 256u32 } else { 65536 };
    for row in samples.chunks(width) {
        let mut previous = 0u16;
        let stored: Vec<u16> = row
            .iter()
            .map(|&sample| {
                let value = if predicted {
                    u16::try_from((u32::from(sample) + modulus - u32::from(previous)) % modulus)
                        .expect("the difference fits the sample size")
                } else {
                    sample
                };
                previous = sample;
                value
            })
            .collect();
        push_samples(&mut out, &stored, depth);
    }
    debug_assert_eq!(out.len(), samples.len() * sample_bytes(depth));
    out
}

/// The bytes that follow the compression code of one channel.
///
/// A layer with `raw_payload` writes those bytes for every channel. Codes 2 and
/// 3 write a real zlib stream. Any other code writes raw samples.
pub fn channel_payload(spec: &LayerSpec, samples: &[u16], depth: u16) -> Vec<u8> {
    if let Some(payload) = &spec.raw_payload {
        return payload.clone();
    }
    let width = usize::try_from(spec.rect.3 - spec.rect.1).expect("width");
    match spec.compression {
        2 | 3 => deflate(&channel_bytes(samples, width, depth, spec.compression == 3)),
        _ => channel_bytes(samples, width.max(1), depth, false),
    }
}
