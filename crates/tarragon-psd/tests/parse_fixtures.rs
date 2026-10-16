//! PSD and PSB parser spike (study task 17).
//!
//! These tests read the committed fixture corpus in `tests/fixtures/psd/`.
//! They prove the acceptance criterion: the header and one layer parse from
//! each fixture under the documented approach in the crate `README.md`.
//!
//! The tests never touch the real user data directory.

use std::path::{Path, PathBuf};

use tarragon_psd::error::PsdError;
use tarragon_psd::parse::{ColorMode, PsdVersion, read_bytes, read_file};

/// The expected shape of one valid fixture.
struct Expected {
    /// The fixture file name.
    file: &'static str,
    /// The file version.
    version: PsdVersion,
    /// The merged channel count.
    channels: u16,
    /// The image height.
    height: u32,
    /// The image width.
    width: u32,
    /// The bit depth.
    depth: u16,
    /// The color mode.
    color_mode: ColorMode,
    /// The layer count.
    layer_count: usize,
    /// The first layer name.
    layer_name: &'static str,
    /// The first layer rectangle: top, left, bottom, right.
    bounds: (i32, i32, i32, i32),
    /// The first layer channel count.
    layer_channels: usize,
}

/// The eight valid fixtures and their real header and first-layer values.
const VALID: [Expected; 8] = [
    Expected {
        file: "rgb_8bit.psd",
        version: PsdVersion::Psd,
        channels: 3,
        height: 64,
        width: 64,
        depth: 8,
        color_mode: ColorMode::Rgb,
        layer_count: 1,
        layer_name: "Background",
        bounds: (0, 0, 64, 64),
        layer_channels: 3,
    },
    Expected {
        file: "rgb_16bit.psd",
        version: PsdVersion::Psd,
        channels: 3,
        height: 48,
        width: 48,
        depth: 16,
        color_mode: ColorMode::Rgb,
        layer_count: 1,
        layer_name: "Deep 16-bit",
        bounds: (0, 0, 48, 48),
        layer_channels: 3,
    },
    Expected {
        file: "cmyk_8bit.psd",
        version: PsdVersion::Psd,
        channels: 4,
        height: 64,
        width: 64,
        depth: 8,
        color_mode: ColorMode::Cmyk,
        layer_count: 1,
        layer_name: "CMYK Ink",
        bounds: (0, 0, 64, 64),
        layer_channels: 4,
    },
    Expected {
        file: "layered_blend_modes.psd",
        version: PsdVersion::Psd,
        channels: 4,
        height: 96,
        width: 96,
        depth: 8,
        color_mode: ColorMode::Rgb,
        layer_count: 5,
        layer_name: "Normal",
        bounds: (0, 0, 96, 96),
        layer_channels: 4,
    },
    Expected {
        file: "layer_groups.psd",
        version: PsdVersion::Psd,
        channels: 3,
        height: 96,
        width: 96,
        depth: 8,
        color_mode: ColorMode::Rgb,
        layer_count: 5,
        layer_name: "Background",
        bounds: (0, 0, 96, 96),
        layer_channels: 3,
    },
    Expected {
        file: "clipping_mask.psd",
        version: PsdVersion::Psd,
        channels: 3,
        height: 96,
        width: 96,
        depth: 8,
        color_mode: ColorMode::Rgb,
        layer_count: 3,
        layer_name: "Base",
        bounds: (0, 0, 96, 96),
        layer_channels: 3,
    },
    Expected {
        file: "layer_mask.psd",
        version: PsdVersion::Psd,
        channels: 3,
        height: 64,
        width: 64,
        depth: 8,
        color_mode: ColorMode::Rgb,
        layer_count: 1,
        layer_name: "Masked",
        bounds: (0, 0, 64, 64),
        layer_channels: 5,
    },
    Expected {
        file: "psb_8bit.psb",
        version: PsdVersion::Psb,
        channels: 3,
        height: 64,
        width: 64,
        depth: 8,
        color_mode: ColorMode::Rgb,
        layer_count: 1,
        layer_name: "PSB Layer",
        bounds: (0, 0, 64, 64),
        layer_channels: 3,
    },
];

/// The absolute path of a committed fixture.
fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/psd")
        .join(name)
}

#[test]
fn every_valid_fixture_parses_its_header_and_first_layer() {
    for case in &VALID {
        let file =
            read_file(&fixture(case.file)).unwrap_or_else(|error| panic!("{}: {error}", case.file));
        assert_eq!(file.header.version, case.version, "{}", case.file);
        assert_eq!(file.header.channels, case.channels, "{}", case.file);
        assert_eq!(file.header.height, case.height, "{}", case.file);
        assert_eq!(file.header.width, case.width, "{}", case.file);
        assert_eq!(file.header.depth, case.depth, "{}", case.file);
        assert_eq!(file.header.color_mode, case.color_mode, "{}", case.file);
        assert_eq!(file.layer_count, case.layer_count, "{}", case.file);

        let layer = file
            .first_layer
            .unwrap_or_else(|| panic!("{}: no first layer", case.file));
        assert_eq!(layer.name, case.layer_name, "{}", case.file);
        let (top, left, bottom, right) = case.bounds;
        assert_eq!(layer.bounds.top, top, "{}", case.file);
        assert_eq!(layer.bounds.left, left, "{}", case.file);
        assert_eq!(layer.bounds.bottom, bottom, "{}", case.file);
        assert_eq!(layer.bounds.right, right, "{}", case.file);
        assert_eq!(layer.channels.len(), case.layer_channels, "{}", case.file);
    }
}

#[test]
fn the_first_channel_offset_points_at_the_rle_marker() {
    for case in &VALID {
        let bytes = std::fs::read(fixture(case.file)).expect("read the fixture");
        let file = read_bytes(&bytes).expect("parse the fixture");
        let channel = file
            .first_layer
            .expect("a first layer")
            .channels
            .first()
            .copied()
            .expect("a first channel");
        let start = usize::try_from(channel.data_offset).expect("offset fits");
        assert!(channel.length > 2, "{}: channel is too short", case.file);
        // Every layer channel in the corpus is RLE compressed.
        assert_eq!(
            bytes.get(start..start + 2),
            Some(&[0, 1][..]),
            "{}",
            case.file
        );
    }
}

#[test]
fn the_psb_reader_uses_eight_byte_lengths() {
    let file = read_file(&fixture("psb_8bit.psb")).expect("parse the PSB");
    assert_eq!(file.header.version, PsdVersion::Psb);
    let layer = file.first_layer.expect("a first layer");
    let lengths: Vec<u64> = layer
        .channels
        .iter()
        .map(|channel| channel.length)
        .collect();
    // The channel length and the section lengths are eight bytes in PSB. A
    // four-byte read would misplace every field after the first channel.
    assert_eq!(lengths, vec![386, 386, 386]);
    assert_eq!(layer.channels[0].data_offset, 306);
}

#[test]
fn the_psd_reader_uses_four_byte_lengths() {
    let file = read_file(&fixture("rgb_8bit.psd")).expect("parse the PSD");
    assert_eq!(file.header.version, PsdVersion::Psd);
    let layer = file.first_layer.expect("a first layer");
    assert_eq!(layer.channels[0].length, 258);
    assert_eq!(layer.channels[0].data_offset, 286);
}

#[test]
fn the_layer_mask_channel_ids_parse() {
    let file = read_file(&fixture("layer_mask.psd")).expect("parse the mask fixture");
    let layer = file.first_layer.expect("a first layer");
    let ids: Vec<i16> = layer.channels.iter().map(|channel| channel.id).collect();
    assert_eq!(ids, vec![0, 1, 2, -1, -2]);
}

#[test]
fn the_truncated_fixture_takes_the_typed_error_path() {
    let result = read_file(&fixture("truncated.psd"));
    assert!(
        matches!(result, Err(PsdError::Truncated { .. })),
        "unexpected result: {result:?}"
    );
}

#[test]
fn truncation_prefixes_never_panic() {
    let bytes = std::fs::read(fixture("layered_blend_modes.psd")).expect("read the fixture");
    // The first 256 bytes cut the header, the resources, and part of layer 0.
    for length in 0..=256 {
        assert!(
            read_bytes(&bytes[..length]).is_err(),
            "prefix {length} parsed without error"
        );
    }
    // A bounded stride over the rest proves no prefix panics.
    for length in (257..bytes.len()).step_by(97) {
        let _ = read_bytes(&bytes[..length]);
    }
    assert!(read_bytes(&bytes).is_ok(), "the full file must parse");
}

#[test]
fn an_empty_slice_is_a_typed_error() {
    let result = read_bytes(b"");
    assert!(
        matches!(result, Err(PsdError::Truncated { .. })),
        "unexpected result: {result:?}"
    );
}
