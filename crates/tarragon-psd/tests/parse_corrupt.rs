//! Corrupt and truncated input to the PSD parser.
//!
//! Each test damages a fixture or a hand-built file and expects a typed error,
//! never a panic or a read of a neighbouring block.

mod common;

use tarragon_psd::decode::Viewport;
use tarragon_psd::error::PsdError;
use tarragon_psd::limits::ParseOptions;
use tarragon_psd::parse::{read_bytes, read_bytes_lenient, read_file};

use common::{
    DocumentSpec, LayerSpec, build_psd, fixture, layered_psd, read_fixture, rle_merged_section,
    section_offset,
};

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
    // The merged image data sits at the end, so no shorter prefix is complete.
    for length in (257..bytes.len()).step_by(97) {
        assert!(
            read_bytes(&bytes[..length]).is_err(),
            "prefix {length} parsed without error"
        );
    }
    assert!(read_bytes(&bytes).is_ok(), "the full file must parse");
}

#[test]
fn a_bounded_sweep_of_corrupt_prefixes_never_panics() {
    let (bytes, file) = read_fixture("layer_mask.psd");
    for length in (0..bytes.len()).step_by(53) {
        let prefix = &bytes[..length];
        assert!(read_bytes(prefix).is_err(), "prefix {length} parsed");
        // The parsed structure over cut data returns a value or a typed error.
        let result =
            file.read_merged_region(prefix, Viewport::new(0, 0, 1, 1), &ParseOptions::none());
        assert!(
            matches!(result, Ok(_) | Err(PsdError::Truncated { .. })),
            "prefix {length}: {result:?}"
        );
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

/// A layered RGB file with an RLE merged section, and where that section starts.
fn layered_with_rle_merged() -> (Vec<u8>, usize) {
    let build = |section: Vec<u8>| {
        let mut spec = DocumentSpec::new(8, 8, 8, 3);
        spec.layers
            .push(LayerSpec::opaque((0, 0, 8, 8), [10, 20, 30]));
        spec.merged_section = Some(section);
        build_psd(&spec)
    };
    let planes: Vec<Vec<u8>> = (0..3u8)
        .map(|seed| {
            (0..64u8)
                .map(|i| i.wrapping_mul(3).wrapping_add(seed))
                .collect()
        })
        .collect();
    let start = build(Vec::new()).len();
    (build(rle_merged_section(8, &planes, false)), start)
}

#[test]
fn the_lenient_read_never_panics_on_truncated_prefixes() {
    let bytes = std::fs::read(fixture("layered_blend_modes.psd")).expect("read the fixture");
    for length in (0..bytes.len()).step_by(41) {
        let _ = read_bytes_lenient(&bytes[..length], &ParseOptions::none());
    }
    assert!(read_bytes_lenient(&bytes, &ParseOptions::none()).is_ok());
}

#[test]
fn a_cut_in_the_layer_section_fails_and_a_cut_in_the_merged_section_opens() {
    let (bytes, merged_start) = layered_with_rle_merged();
    let options = ParseOptions::none();
    for length in 0..bytes.len() {
        let result = read_bytes_lenient(&bytes[..length], &options);
        if length < merged_start {
            assert!(result.is_err(), "prefix {length} inside the layers parsed");
        } else {
            let file = result.unwrap_or_else(|e| panic!("prefix {length}: {e}"));
            assert!(file.merged_error.is_some(), "prefix {length}");
            assert!(file.merged.channels.is_empty(), "prefix {length}");
            assert_eq!(file.layers.len(), 1, "prefix {length}");
        }
    }
    let whole = read_bytes_lenient(&bytes, &options).expect("the whole file parses");
    assert!(whole.merged_error.is_none());
    assert_eq!(whole.merged.channels.len(), 3);
}
