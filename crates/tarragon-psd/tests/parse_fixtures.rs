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

use common::fixture;

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

/// Read a fixture's bytes and parsed structure.
fn read_fixture(name: &str) -> (Vec<u8>, tarragon_psd::PsdFile) {
    let bytes = std::fs::read(fixture(name)).unwrap_or_else(|error| panic!("{name}: {error}"));
    let file = read_bytes(&bytes).unwrap_or_else(|error| panic!("{name}: {error}"));
    (bytes, file)
}

/// Build a minimal raw-compression PSD for the given shape.
fn raw_psd(width: u32, height: u32, channels: u16, mode: u16, planes: &[Vec<u8>]) -> Vec<u8> {
    let mut data = Vec::new();
    data.extend_from_slice(b"8BPS");
    data.extend_from_slice(&1u16.to_be_bytes());
    data.extend_from_slice(&[0, 0, 0, 0, 0, 0]);
    data.extend_from_slice(&channels.to_be_bytes());
    data.extend_from_slice(&height.to_be_bytes());
    data.extend_from_slice(&width.to_be_bytes());
    data.extend_from_slice(&8u16.to_be_bytes());
    data.extend_from_slice(&mode.to_be_bytes());
    // Empty color-mode data and image resources.
    data.extend_from_slice(&0u32.to_be_bytes());
    data.extend_from_slice(&0u32.to_be_bytes());
    // An empty layer-and-mask section.
    data.extend_from_slice(&0u32.to_be_bytes());
    // A raw merged image.
    data.extend_from_slice(&0u16.to_be_bytes());
    for plane in planes {
        data.extend_from_slice(plane);
    }
    data
}

/// The file offset of the layer-and-mask section length field.
fn section_offset(bytes: &[u8]) -> usize {
    let color = u32::from_be_bytes([bytes[26], bytes[27], bytes[28], bytes[29]]) as usize;
    let resources_at = 30 + color;
    let resources = u32::from_be_bytes([
        bytes[resources_at],
        bytes[resources_at + 1],
        bytes[resources_at + 2],
        bytes[resources_at + 3],
    ]) as usize;
    resources_at + 4 + resources
}

/// Build a minimal PSD with one layer and the given channel blocks.
///
/// Each channel is `(id, declared_length, block)`. The block bytes are stored
/// in the file, and the declared length is written into the record. A declared
/// length that differs from the block is the corruption under test.
fn layered_psd(width: u32, height: u32, channels: &[(i16, u32, Vec<u8>)]) -> Vec<u8> {
    let mut data = Vec::new();
    // Header: 1x1 or width x height RGB, 8-bit, three merged channels.
    data.extend_from_slice(b"8BPS");
    data.extend_from_slice(&1u16.to_be_bytes());
    data.extend_from_slice(&[0, 0, 0, 0, 0, 0]);
    data.extend_from_slice(&3u16.to_be_bytes());
    data.extend_from_slice(&height.to_be_bytes());
    data.extend_from_slice(&width.to_be_bytes());
    data.extend_from_slice(&8u16.to_be_bytes());
    data.extend_from_slice(&3u16.to_be_bytes());
    // Empty color-mode data and image resources.
    data.extend_from_slice(&0u32.to_be_bytes());
    data.extend_from_slice(&0u32.to_be_bytes());

    // The extra data: mask length, blending ranges length, pascal name.
    let mut extra = Vec::new();
    extra.extend_from_slice(&0u32.to_be_bytes());
    extra.extend_from_slice(&0u32.to_be_bytes());
    extra.extend_from_slice(&[0, 0, 0, 0]);

    // One layer record.
    let mut record = Vec::new();
    record.extend_from_slice(&0i32.to_be_bytes());
    record.extend_from_slice(&0i32.to_be_bytes());
    record.extend_from_slice(
        &i32::try_from(height)
            .expect("height fits i32")
            .to_be_bytes(),
    );
    record.extend_from_slice(&i32::try_from(width).expect("width fits i32").to_be_bytes());
    record.extend_from_slice(
        &u16::try_from(channels.len())
            .expect("channel count fits u16")
            .to_be_bytes(),
    );
    for (id, declared, _) in channels {
        record.extend_from_slice(&id.to_be_bytes());
        record.extend_from_slice(&declared.to_be_bytes());
    }
    record.extend_from_slice(b"8BIM");
    record.extend_from_slice(b"norm");
    record.push(255);
    record.push(0);
    record.push(0);
    record.push(0);
    record.extend_from_slice(
        &u32::try_from(extra.len())
            .expect("extra fits u32")
            .to_be_bytes(),
    );
    record.extend_from_slice(&extra);

    // Layer info: count, record, then the channel data blocks.
    let mut info = Vec::new();
    info.extend_from_slice(&1i16.to_be_bytes());
    info.extend_from_slice(&record);
    for (_, _, block) in channels {
        info.extend_from_slice(block);
    }

    // The layer-and-mask section: layer_info_length, info, global mask length.
    data.extend_from_slice(
        &u32::try_from(4 + info.len() + 4)
            .expect("section fits u32")
            .to_be_bytes(),
    );
    data.extend_from_slice(
        &u32::try_from(info.len())
            .expect("info fits u32")
            .to_be_bytes(),
    );
    data.extend_from_slice(&info);
    data.extend_from_slice(&0u32.to_be_bytes());

    // Merged image: raw, three channels.
    data.extend_from_slice(&0u16.to_be_bytes());
    data.extend(std::iter::repeat_n(
        0u8,
        usize::try_from(width * height * 3).unwrap_or(0),
    ));
    data
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
        // Every layer channel in the corpus is RLE compressed.
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
    // Grayscale has no channel reader in v0.2.
    let data = raw_psd(2, 1, 1, 1, &[vec![0, 1]]);
    let file = read_bytes(&data).expect("the structure parses");
    let error = file
        .read_merged_region(&data, Viewport::full(2, 1), &ParseOptions::none())
        .expect_err("grayscale is unsupported");
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

#[test]
fn the_truncated_fixture_takes_the_typed_error_path() {
    let result = read_file(&fixture("truncated.psd"));
    assert!(
        matches!(result, Err(PsdError::Truncated { .. })),
        "unexpected result: {result:?}"
    );
}

#[test]
fn a_corrupt_section_length_is_rejected() {
    let (mut bytes, _) = read_fixture("rgb_8bit.psd");
    let offset = section_offset(&bytes);

    // A section length past the end of the file is a typed error.
    bytes[offset..offset + 4].copy_from_slice(&0xFFFF_FFF0u32.to_be_bytes());
    assert!(matches!(
        read_bytes(&bytes),
        Err(PsdError::Truncated { .. })
    ));

    // A layer-info length past the section end is a typed error.
    let (mut bytes, _) = read_fixture("rgb_8bit.psd");
    bytes[offset + 4..offset + 8].copy_from_slice(&0xFFFF_FFF0u32.to_be_bytes());
    assert!(matches!(
        read_bytes(&bytes),
        Err(PsdError::Truncated { .. })
    ));

    // A corrupt layer count that cannot fit in the section is rejected before
    // any large allocation.
    let (mut bytes, _) = read_fixture("rgb_8bit.psd");
    bytes[offset + 8..offset + 10].copy_from_slice(&0x7FFFu16.to_be_bytes());
    assert!(matches!(
        read_bytes(&bytes),
        Err(PsdError::Truncated { .. })
    ));

    // A layer-info length that cuts a record short, while the section still
    // holds the bytes. The record walk is bounded by layer_info_length alone.
    let (mut bytes, _) = read_fixture("rgb_8bit.psd");
    bytes[offset + 4..offset + 8].copy_from_slice(&40u32.to_be_bytes());
    assert!(matches!(
        read_bytes(&bytes),
        Err(PsdError::Truncated { .. })
    ));
}

#[test]
fn a_short_rle_channel_block_is_bounded() {
    // A block that cannot hold its row table is rejected at parse time.
    let too_small = layered_psd(1, 1, &[(0, 2, vec![0, 1])]);
    assert!(matches!(
        read_bytes(&too_small),
        Err(PsdError::Truncated { .. })
    ));

    // A block that holds the table but whose row length exceeds its data is
    // refused at decode time, so it cannot read the next channel's bytes. The
    // second channel fills the file with bytes a stale decoder would consume.
    let mut block = vec![0, 1, 0x00, 0xC8];
    block.resize(10, 0x7F);
    let data = layered_psd(1, 1, &[(0, 10, block), (1, 300, vec![0xAA; 300])]);
    let file = read_bytes(&data).expect("the structure parses");
    let error = file
        .read_layer_region(&data, 0, Viewport::full(1, 1), &ParseOptions::none())
        .expect_err("the row length exceeds the block");
    assert!(matches!(error, PsdError::Truncated { .. }));
}

#[test]
fn a_zero_length_channel_decodes_to_zeros() {
    let data = layered_psd(1, 1, &[(0, 0, Vec::new())]);
    let file = read_bytes(&data).expect("the zero-length channel parses");
    let region = file
        .read_layer_region(&data, 0, Viewport::full(1, 1), &ParseOptions::none())
        .expect("the zero-length channel decodes");
    assert_eq!(region.channels.len(), 1);
    assert_eq!(region.sample(0, 0, 0), Some(0));
}

#[test]
fn a_zero_length_channel_with_unsupported_compression_is_rejected() {
    // The two-byte block is a compression field with no sample data. A code
    // outside raw and RLE must error, not return zeros. Check the compression
    // code before the empty-block return, or the test returns Ok.
    let data = layered_psd(1, 1, &[(0, 2, vec![0, 2])]);
    let file = read_bytes(&data).expect("the two-byte channel parses");
    let error = file
        .read_layer_region(&data, 0, Viewport::full(1, 1), &ParseOptions::none())
        .expect_err("the unsupported compression is rejected");
    assert!(matches!(error, PsdError::UnsupportedCompression(2)));
}

#[test]
fn a_sub_two_byte_channel_does_not_read_its_block() {
    // A one-byte block has no room for the compression field. The decoder must
    // ignore the nonzero byte and return zeros, not read it as a raw sample.
    let data = layered_psd(1, 1, &[(0, 1, vec![0xAB])]);
    let file = read_bytes(&data).expect("the one-byte channel parses");
    let region = file
        .read_layer_region(&data, 0, Viewport::full(1, 1), &ParseOptions::none())
        .expect("the one-byte channel decodes");
    assert_eq!(region.channels.len(), 1);
    assert_eq!(region.sample(0, 0, 0), Some(0));
}

#[test]
fn a_raw_channel_shorter_than_a_row_is_rejected() {
    // The block holds the two-byte compression field and one data byte, but a
    // 2x1 raw row needs two bytes. The decoder must refuse the short row.
    let data = layered_psd(2, 1, &[(0, 3, vec![0, 0, 0xAB])]);
    let file = read_bytes(&data).expect("the raw channel parses");
    let error = file
        .read_layer_region(&data, 0, Viewport::full(2, 1), &ParseOptions::none())
        .expect_err("the raw row is shorter than row_bytes");
    assert!(matches!(error, PsdError::Truncated { .. }));
}

#[test]
fn a_huge_layer_rectangle_is_rejected() {
    let (mut bytes, _) = read_fixture("rgb_8bit.psd");
    let offset = section_offset(&bytes);
    // The right edge of layer 0: section, layer-info length, count, 4 bounds.
    let right = offset + 4 + 4 + 2 + 12;
    bytes[right..right + 4].copy_from_slice(&0x7FFF_FFFFu32.to_be_bytes());
    let error = read_bytes(&bytes).expect_err("the layer rectangle is too wide");
    assert!(matches!(error, PsdError::BadDimensions { .. }));
}

#[test]
fn a_truncated_rle_row_table_is_rejected() {
    let (mut bytes, _) = read_fixture("rgb_8bit.psd");
    // Cut the file inside the merged image row table. The parse must return a
    // typed error and must not allocate the huge table.
    let short = bytes.split_off(bytes.len() - 200);
    drop(short);
    assert!(matches!(
        read_bytes(&bytes),
        Err(PsdError::Truncated { .. })
    ));
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
fn a_bounded_sweep_of_corrupt_prefixes_never_panics() {
    let (bytes, _) = read_fixture("layer_mask.psd");
    for length in (0..bytes.len()).step_by(53) {
        let prefix = &bytes[..length];
        if let Ok(file) = read_bytes(prefix) {
            // A parsed prefix must still decode a region without panicking.
            let _ =
                file.read_merged_region(prefix, Viewport::new(0, 0, 1, 1), &ParseOptions::none());
        }
    }
}

#[test]
fn an_empty_slice_is_a_typed_error() {
    let result = read_bytes(b"");
    assert!(
        matches!(result, Err(PsdError::Truncated { .. })),
        "unexpected result: {result:?}"
    );
}
