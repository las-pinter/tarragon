//! Tests for the metadata text.

use super::*;

#[test]
fn size_text_has_exact_output_at_the_unit_boundaries() {
    let cases: [(u64, &str); 14] = [
        (0, "0 B"),
        (1, "1 B"),
        (1023, "1023 B"),
        (1024, "1.0 KB"),
        (1280, "1.3 KB"),
        (1536, "1.5 KB"),
        (1_048_575, "1.0 MB"),
        (1_048_576, "1.0 MB"),
        (2_516_582, "2.4 MB"),
        (1_073_741_823, "1.0 GB"),
        (1_073_741_824, "1.0 GB"),
        (5_368_709_120, "5.0 GB"),
        (1_099_511_627_776, "1024.0 GB"),
        (u64::MAX, "17179869184.0 GB"),
    ];
    for (bytes, want) in cases {
        assert_eq!(size_text(bytes), want, "{bytes} bytes");
    }
}

#[test]
fn dimensions_text_joins_width_and_height() {
    assert_eq!(dimensions_text((2048, 1536)), "2048 × 1536");
    assert_eq!(dimensions_text((0, 0)), "0 × 0");
}

#[test]
fn the_line_joins_the_known_values_and_leaves_out_the_rest() {
    let meta = Meta {
        name: "sketch_014.png".into(),
        dimensions: Some((1536, 2048)),
        size: Some(2_516_582),
        format: "PNG".into(),
        modified: Some("Oct 14, 2026".into()),
    };
    assert_eq!(meta.line(), "1536 × 2048 · 2.4 MB · PNG · Oct 14, 2026");
    let bare = Meta {
        dimensions: None,
        size: None,
        modified: None,
        ..meta
    };
    assert_eq!(bare.line(), "PNG");
}
