//! PSD and PSB parser behavior.
//!
//! These tests read the committed fixture corpus in `tests/fixtures/psd/`.
//! They prove the parser contract: every fixture parses every layer,
//! the tiled viewport read returns the right pixels, the 8192 px cap is
//! enforced, and the 120 s deadline returns a typed timeout.
//!
//! The tests never touch the real user data directory.

mod common;

use std::time::Duration;

use tarragon_psd::PsdDocument;
use tarragon_psd::decode::Viewport;
use tarragon_psd::error::PsdError;
use tarragon_psd::limits::{Deadline, FULL_TIER_MAX_EDGE, ParseOptions, full_tier_size};
use tarragon_psd::parse::{Clipping, ColorMode, Compression, PsdVersion, read_bytes, read_file};

use common::{fixture, raw_psd, read_fixture};

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

/// The nine valid fixtures and their real header and first-layer values.
const VALID: [Expected; 9] = [
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
        file: "layer_effects.psd",
        version: PsdVersion::Psd,
        channels: 3,
        height: 192,
        width: 192,
        depth: 8,
        color_mode: ColorMode::Rgb,
        layer_count: 10,
        layer_name: "Backdrop",
        bounds: (0, 0, 192, 192),
        layer_channels: 3,
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
            .first_layer()
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
fn every_valid_fixture_parses_all_layer_records() {
    for case in &VALID {
        let (_, file) = read_fixture(case.file);
        assert_eq!(file.layers.len(), case.layer_count, "{}", case.file);
        for (index, layer) in file.layers.iter().enumerate() {
            assert!(
                !layer.name.is_empty() || layer.bounds.width() == 0,
                "{} layer {index}: empty name",
                case.file
            );
        }
        // The layer tree and the merged image always parse together.
        assert!(!file.merged.channels.is_empty(), "{}", case.file);
    }
}

#[test]
fn the_first_channel_offset_points_at_the_rle_marker() {
    for case in &VALID {
        let (bytes, file) = read_fixture(case.file);
        let channel = file
            .first_layer()
            .expect("a first layer")
            .channels
            .first()
            .copied()
            .expect("a first channel");
        let start = usize::try_from(channel.data_offset).expect("offset fits");
        assert!(channel.length > 2, "{}: channel is too short", case.file);
        // The layer-effects fixture is raw-compressed: its Rust builder writes
        // raw channel rows.
        if case.file == "layer_effects.psd" {
            assert_eq!(channel.compression, 0, "{}", case.file);
            continue;
        }
        // Every other layer channel in the corpus is RLE compressed.
        assert_eq!(
            bytes.get(start..start + 2),
            Some(&[0, 1][..]),
            "{}",
            case.file
        );
        assert_eq!(channel.compression, 1, "{}", case.file);
    }
}

#[test]
fn the_psb_reader_uses_eight_byte_lengths() {
    let file = read_file(&fixture("psb_8bit.psb")).expect("parse the PSB");
    assert_eq!(file.header.version, PsdVersion::Psb);
    let layer = file.first_layer().expect("a first layer");
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
    let layer = file.first_layer().expect("a first layer");
    assert_eq!(layer.channels[0].length, 258);
    assert_eq!(layer.channels[0].data_offset, 286);
}

#[test]
fn the_layer_mask_channel_ids_parse() {
    let file = read_file(&fixture("layer_mask.psd")).expect("parse the mask fixture");
    let layer = file.first_layer().expect("a first layer");
    let ids: Vec<i16> = layer.channels.iter().map(|channel| channel.id).collect();
    assert_eq!(ids, vec![0, 1, 2, -1, -2]);

    let mask = layer.mask.expect("the layer carries a mask");
    assert_eq!(mask.bounds.width(), 64);
    assert_eq!(mask.bounds.height(), 64);
    // The user mask channel covers the mask rectangle.
    let channel = layer.channels[4];
    assert_eq!(channel.id, -2);
    assert_eq!(channel.bounds.width(), 64);
    assert_eq!(channel.bounds.height(), 64);
}

#[test]
fn the_group_fixture_records_the_section_divider() {
    let file = read_file(&fixture("layer_groups.psd")).expect("parse the group fixture");
    let opens: Vec<usize> = file
        .layers
        .iter()
        .enumerate()
        .filter(|(_, layer)| {
            layer
                .section
                .is_some_and(tarragon_psd::LayerSection::opens_group)
        })
        .map(|(index, _)| index)
        .collect();
    let closes: Vec<usize> = file
        .layers
        .iter()
        .enumerate()
        .filter(|(_, layer)| {
            layer
                .section
                .is_some_and(tarragon_psd::LayerSection::closes_group)
        })
        .map(|(index, _)| index)
        .collect();
    assert_eq!(opens, vec![1]);
    assert_eq!(closes, vec![4]);
    assert_eq!(file.layers[1].channels.len(), 0);
    assert_eq!(file.layers[4].channels.len(), 0);
}

#[test]
fn the_clipping_fixture_records_the_clipping_flag() {
    let file = read_file(&fixture("clipping_mask.psd")).expect("parse the clipping fixture");
    assert_eq!(file.layers[0].clipping, Clipping::Base);
    assert_eq!(file.layers[1].clipping, Clipping::NonBase);
    assert_eq!(file.layers[2].clipping, Clipping::NonBase);
}

#[test]
fn the_layer_blending_ranges_parse() {
    let file = read_file(&fixture("rgb_8bit.psd")).expect("parse the fixture");
    let ranges = &file.first_layer().expect("a first layer").blending_ranges;
    assert_eq!(ranges.composite, [0, 65535, 0, 65535]);
    assert_eq!(ranges.channels.len(), 4);
    assert_eq!(ranges.channels[0], [0, 65535, 0, 65535]);
}

#[test]
fn a_fixture_with_no_global_mask_reports_none() {
    let (_, file) = read_fixture("rgb_8bit.psd");
    assert!(file.global_mask.is_none());
}

#[test]
fn a_viewport_smaller_than_the_canvas_yields_the_right_pixels() {
    let document = PsdDocument::open(&fixture("rgb_8bit.psd")).expect("open the fixture");
    let options = ParseOptions::none();
    // The merged image is the generator's gradient: (x*4, y*4, (x+y)*2).
    let region = document
        .read_merged_region(Viewport::new(10, 5, 20, 15), &options)
        .expect("read the viewport");
    assert_eq!(region.channels.len(), 3);
    assert_eq!(region.channels[0].samples.len(), 10 * 10);
    assert_eq!(region.sample(0, 0, 0), Some(10 * 4));
    assert_eq!(region.sample(1, 0, 0), Some(5 * 4));
    assert_eq!(region.sample(2, 0, 0), Some((10 + 5) * 2));
    // The last pixel of the viewport.
    assert_eq!(region.sample(0, 9, 9), Some((19 * 4) & 0xFF));
    assert_eq!(region.sample(1, 9, 9), Some((14 * 4) & 0xFF));

    // The layer is a solid (200, 60, 30).
    let layer = document
        .read_layer_region(0, Viewport::new(0, 0, 4, 4), &options)
        .expect("read the layer viewport");
    assert_eq!(layer.sample(0, 0, 0), Some(200));
    assert_eq!(layer.sample(1, 0, 0), Some(60));
    assert_eq!(layer.sample(2, 0, 0), Some(30));
}

#[test]
fn a_16_bit_merged_region_decodes() {
    let document = PsdDocument::open(&fixture("rgb_16bit.psd")).expect("open the fixture");
    let region = document
        .read_merged_region(Viewport::new(10, 5, 11, 6), &ParseOptions::none())
        .expect("read the 16-bit viewport");
    assert_eq!(region.depth, 16);
    assert_eq!(region.sample(0, 0, 0), Some(10 * 800));
    assert_eq!(region.sample(1, 0, 0), Some(5 * 800));
    assert_eq!(region.sample(2, 0, 0), Some(15 * 400));
}

#[test]
fn a_cmyk_merged_region_decodes() {
    let document = PsdDocument::open(&fixture("cmyk_8bit.psd")).expect("open the fixture");
    let region = document
        .read_merged_region(Viewport::new(0, 0, 2, 2), &ParseOptions::none())
        .expect("read the CMYK viewport");
    assert_eq!(region.color_mode, ColorMode::Cmyk);
    let ids: Vec<i16> = region.channels.iter().map(|channel| channel.id).collect();
    assert_eq!(ids, vec![0, 1, 2, 3]);
    assert_eq!(region.sample(0, 0, 0), Some(30));
    assert_eq!(region.sample(1, 0, 0), Some(60));
    assert_eq!(region.sample(2, 0, 0), Some(90));
    assert_eq!(region.sample(3, 0, 0), Some(10));
}

#[test]
fn a_raw_merged_image_decodes() {
    let data = raw_psd(
        2,
        2,
        3,
        3,
        &[vec![1, 2, 3, 4], vec![5, 6, 7, 8], vec![9, 10, 11, 12]],
    );
    let file = read_bytes(&data).expect("parse the raw file");
    assert_eq!(file.merged.compression, Compression::Raw);
    let region = file
        .read_merged_region(&data, Viewport::new(1, 0, 2, 2), &ParseOptions::none())
        .expect("read the raw region");
    assert_eq!(region.sample(0, 0, 0), Some(2));
    assert_eq!(region.sample(0, 0, 1), Some(4));
    assert_eq!(region.sample(1, 0, 1), Some(8));
    assert_eq!(region.sample(2, 0, 0), Some(10));
}

#[test]
fn an_unsupported_color_mode_is_a_typed_error() {
    // Lab has no channel reader.
    let data = raw_psd(2, 1, 3, 9, &[vec![0, 1], vec![0, 1], vec![0, 1]]);
    let file = read_bytes(&data).expect("the structure parses");
    let error = file
        .read_merged_region(&data, Viewport::full(2, 1), &ParseOptions::none())
        .expect_err("lab is unsupported");
    assert!(matches!(error, PsdError::UnsupportedColorMode { .. }));
}

#[test]
fn the_full_tier_cap_is_enforced() {
    // The cap clamps the full-tier target.
    assert_eq!(full_tier_size(9000, 9000), (8192, 8192));
    assert_eq!(full_tier_size(100, 50), (100, 50));

    let document = PsdDocument::open(&fixture("rgb_8bit.psd")).expect("open the fixture");
    let error = document
        .read_merged_region(
            Viewport::new(0, 0, FULL_TIER_MAX_EDGE + 1, FULL_TIER_MAX_EDGE + 1),
            &ParseOptions::none(),
        )
        .expect_err("the region exceeds the cap");
    assert!(matches!(
        error,
        PsdError::CapExceeded { edge, cap } if edge == FULL_TIER_MAX_EDGE + 1 && cap == FULL_TIER_MAX_EDGE
    ));
}

#[test]
fn an_expired_deadline_returns_the_typed_timeout() {
    let document = PsdDocument::open(&fixture("rgb_8bit.psd")).expect("open the fixture");
    let expired = ParseOptions::with_deadline(Deadline::after(Duration::from_secs(0)));
    let error = document
        .read_merged_region(Viewport::full(64, 64), &expired)
        .expect_err("the deadline is expired");
    assert!(matches!(error, PsdError::Timeout));
}
