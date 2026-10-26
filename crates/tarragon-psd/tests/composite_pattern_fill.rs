//! Pattern fill layers (`PtFl`) and the document pattern table.

mod common;

use common::{
    DocumentSpec, LayerSpec, PatternSpec, PtflSpec, build_psd, composite_bytes, pattern_record,
    ptfl_block, sample_rgba8, tagged_block,
};
use tarragon_psd::{LayerContent, ParseOptions, PsdDocument, Viewport};

/// A 4x4 RGB tile where every pixel is distinct.
fn tile4(id: &str) -> PatternSpec {
    let pixels: Vec<[u8; 3]> = (0..16u8)
        .map(|index| {
            let (x, y) = (index % 4, index / 4);
            [index * 10 + 5, x * 60 + 3, y * 60 + 7]
        })
        .collect();
    PatternSpec::rgb8(id, 4, 4, &pixels)
}

/// The expected tile pixel at `(column, row)` of [`tile4`].
fn tile4_pixel(column: i64, row: i64) -> [u8; 3] {
    let (x, y) = (column.rem_euclid(4), row.rem_euclid(4));
    let index = u8::try_from(y * 4 + x).expect("index");
    [
        index * 10 + 5,
        u8::try_from(x * 60 + 3).expect("green"),
        u8::try_from(y * 60 + 7).expect("blue"),
    ]
}

/// An 8-bit document with a pattern table and one pattern fill layer.
fn document(patterns: &[PatternSpec], fill: &PtflSpec, rect: (i32, i32, i32, i32)) -> DocumentSpec {
    let mut table = Vec::new();
    for pattern in patterns {
        table.extend_from_slice(&pattern_record(pattern));
    }
    let mut spec = DocumentSpec::new(8, 8, 8, 3);
    spec.layers
        .push(LayerSpec::content(rect).with_block(*b"PtFl", ptfl_block(fill)));
    spec.extra_global_blocks = tagged_block(*b"Patt", &table);
    spec
}

/// Composite a document with one fill layer over the full canvas.
fn render(spec: &DocumentSpec) -> tarragon_psd::CompositeRegion {
    composite_bytes(build_psd(spec))
}

fn rgb(region: &tarragon_psd::CompositeRegion, x: u32, y: u32) -> [u8; 3] {
    let pixel = sample_rgba8(region, x, y);
    [pixel[0], pixel[1], pixel[2]]
}

/// The content of the first layer after a parse.
fn first_content(spec: &DocumentSpec) -> LayerContent {
    let document = PsdDocument::from_bytes(build_psd(spec), &ParseOptions::none()).expect("parses");
    document.file().layers[0].content.clone()
}

#[test]
fn a_tile_at_one_hundred_percent_repeats_exactly() {
    let spec = document(&[tile4("a")], &PtflSpec::new("a"), (0, 0, 8, 8));
    let region = render(&spec);
    for y in 0..8u32 {
        for x in 0..8u32 {
            assert_eq!(
                rgb(&region, x, y),
                tile4_pixel(i64::from(x), i64::from(y)),
                "({x}, {y})"
            );
            assert_eq!(sample_rgba8(&region, x, y)[3], 255);
        }
    }
}

#[test]
fn the_phase_shifts_the_tiling() {
    let mut fill = PtflSpec::new("a");
    fill.phase = (1.0, 2.0);
    let region = render(&document(&[tile4("a")], &fill, (0, 0, 8, 8)));
    for y in 0..8u32 {
        for x in 0..8u32 {
            assert_eq!(
                rgb(&region, x, y),
                tile4_pixel(i64::from(x) - 1, i64::from(y) - 2),
                "({x}, {y})"
            );
        }
    }
}

#[test]
fn a_linked_pattern_starts_at_the_layer_and_an_unlinked_one_at_the_document() {
    let rect = (1, 2, 8, 8);
    let linked = render(&document(&[tile4("a")], &PtflSpec::new("a"), rect));
    assert_eq!(rgb(&linked, 2, 1), tile4_pixel(0, 0));
    assert_eq!(rgb(&linked, 5, 4), tile4_pixel(3, 3));
    let mut fill = PtflSpec::new("a");
    fill.link = Some((*b"Algn", false));
    let unlinked = render(&document(&[tile4("a")], &fill, rect));
    assert_eq!(rgb(&unlinked, 2, 1), tile4_pixel(2, 1));
    assert_eq!(rgb(&unlinked, 5, 4), tile4_pixel(5, 4));
}

#[test]
fn the_lnkd_key_also_sets_the_link_flag() {
    let mut fill = PtflSpec::new("a");
    fill.link = Some((*b"Lnkd", false));
    let region = render(&document(&[tile4("a")], &fill, (1, 2, 8, 8)));
    assert_eq!(rgb(&region, 2, 1), tile4_pixel(2, 1));
    // A missing flag means linked.
    fill.link = None;
    let region = render(&document(&[tile4("a")], &fill, (1, 2, 8, 8)));
    assert_eq!(rgb(&region, 2, 1), tile4_pixel(0, 0));
}

#[test]
fn a_scale_of_two_hundred_percent_doubles_the_period() {
    let mut fill = PtflSpec::new("a");
    fill.scale = 200.0;
    // The period is 8 pixels, so x and x + 8 agree and x and x + 4 differ.
    let wide = {
        let mut spec = DocumentSpec::new(16, 8, 8, 3);
        let mut table = Vec::new();
        table.extend_from_slice(&pattern_record(&tile4("a")));
        spec.layers
            .push(LayerSpec::content((0, 0, 8, 16)).with_block(*b"PtFl", ptfl_block(&fill)));
        spec.extra_global_blocks = tagged_block(*b"Patt", &table);
        render(&spec)
    };
    assert_eq!(rgb(&wide, 2, 1), rgb(&wide, 10, 1));
    assert_ne!(rgb(&wide, 2, 1), rgb(&wide, 6, 1));
}

#[test]
fn an_angle_of_ninety_degrees_rotates_the_tile() {
    let mut fill = PtflSpec::new("a");
    fill.angle = 90.0;
    let region = render(&document(&[tile4("a")], &fill, (0, 0, 8, 8)));
    for y in 0..8u32 {
        for x in 0..8u32 {
            let want = tile4_pixel(-i64::from(y) - 1, i64::from(x));
            let got = rgb(&region, x, y);
            for channel in 0..3 {
                assert!(
                    got[channel].abs_diff(want[channel]) <= 1,
                    "({x}, {y}) {got:?} vs {want:?}"
                );
            }
        }
    }
}

#[test]
fn grey_patterns_decode_at_both_depths_raw_and_rle() {
    for depth in [8u16, 16] {
        for rle in [false, true] {
            let samples: Vec<u16> = [10u16, 20, 30, 40]
                .iter()
                .map(|value| if depth == 8 { *value } else { *value * 257 })
                .collect();
            let mut pattern = PatternSpec::grey("g", 2, 2, depth, &samples);
            pattern.rle = rle;
            let region = render(&document(&[pattern], &PtflSpec::new("g"), (0, 0, 8, 8)));
            assert_eq!(rgb(&region, 0, 0), [10; 3], "depth {depth} rle {rle}");
            assert_eq!(rgb(&region, 1, 0), [20; 3], "depth {depth} rle {rle}");
            assert_eq!(rgb(&region, 0, 1), [30; 3], "depth {depth} rle {rle}");
            assert_eq!(rgb(&region, 3, 3), [40; 3], "depth {depth} rle {rle}");
        }
    }
}

#[test]
fn rle_runs_decode_for_rgb() {
    let mut pattern = PatternSpec::rgb8("r", 8, 2, &[[9, 8, 7]; 16]);
    pattern.rle = true;
    let region = render(&document(&[pattern], &PtflSpec::new("r"), (0, 0, 8, 8)));
    assert_eq!(rgb(&region, 5, 5), [9, 8, 7]);
}

#[test]
fn an_indexed_pattern_looks_up_its_palette() {
    let mut palette = vec![0u8; 768];
    palette[3..6].copy_from_slice(&[200, 100, 50]);
    palette[6..9].copy_from_slice(&[1, 2, 3]);
    let pattern = PatternSpec {
        mode: 2,
        planes: vec![vec![1, 2, 2, 1]],
        palette: Some(palette),
        ..PatternSpec::rgb8("i", 2, 2, &[])
    };
    let region = render(&document(&[pattern], &PtflSpec::new("i"), (0, 0, 8, 8)));
    assert_eq!(rgb(&region, 0, 0), [200, 100, 50]);
    assert_eq!(rgb(&region, 1, 0), [1, 2, 3]);
}

#[test]
fn a_pattern_alpha_channel_makes_the_fill_transparent() {
    let mut pattern = PatternSpec::rgb8("t", 2, 1, &[[100, 150, 200], [10, 20, 30]]);
    pattern.alpha = Some(vec![255, 0]);
    let region = render(&document(&[pattern], &PtflSpec::new("t"), (0, 0, 8, 8)));
    assert_eq!(sample_rgba8(&region, 0, 0), [100, 150, 200, 255]);
    assert_eq!(sample_rgba8(&region, 1, 0)[3], 0);
    let mut half = PatternSpec::rgb8("h", 1, 1, &[[100, 150, 200]]);
    half.alpha = Some(vec![128]);
    let region = render(&document(&[half], &PtflSpec::new("h"), (0, 0, 8, 8)));
    let pixel = sample_rgba8(&region, 3, 3);
    assert_eq!(pixel[3], 128);
    assert!(pixel[0].abs_diff(100) <= 1, "{pixel:?}");
}

#[test]
fn a_channel_smaller_than_the_pattern_sits_at_its_offset() {
    let mut pattern = PatternSpec::grey("o", 4, 4, 8, &[200, 210, 220, 230]);
    pattern.channel_rect = Some((1, 2, 3, 4));
    let region = render(&document(&[pattern], &PtflSpec::new("o"), (0, 0, 8, 8)));
    assert_eq!(rgb(&region, 2, 1), [200; 3]);
    assert_eq!(rgb(&region, 3, 2), [230; 3]);
    assert_eq!(rgb(&region, 0, 0), [0; 3]);
}

#[test]
fn the_pattern_name_matches_when_the_id_does_not() {
    let mut fill = PtflSpec::new("missing");
    fill.name = "name a".to_owned();
    let region = render(&document(&[tile4("a")], &fill, (0, 0, 8, 8)));
    assert_eq!(rgb(&region, 1, 1), tile4_pixel(1, 1));
}

#[test]
fn a_missing_pattern_leaves_the_layer_skipped() {
    let spec = document(&[tile4("a")], &PtflSpec::new("zzz"), (0, 0, 8, 8));
    assert_eq!(first_content(&spec), LayerContent::Unsupported);
    let region = render(&spec);
    assert_eq!(sample_rgba8(&region, 1, 1)[3], 0);
    // A file with no pattern table at all behaves the same.
    let spec = document(&[], &PtflSpec::new("a"), (0, 0, 8, 8));
    assert_eq!(first_content(&spec), LayerContent::Unsupported);
}

#[test]
fn a_truncated_record_drops_only_that_pattern() {
    let mut table = Vec::new();
    // A record whose body stops in the middle of the channel list.
    let body = common::pattern_body(&tile4("bad"));
    let cut = &body[..body.len() / 2];
    table.extend_from_slice(&u32::try_from(cut.len()).expect("length").to_be_bytes());
    table.extend_from_slice(cut);
    table.extend(std::iter::repeat_n(0u8, (4 - cut.len() % 4) % 4));
    table.extend_from_slice(&pattern_record(&tile4("good")));
    let mut spec = DocumentSpec::new(8, 8, 8, 3);
    spec.layers.push(
        LayerSpec::content((0, 0, 8, 8)).with_block(*b"PtFl", ptfl_block(&PtflSpec::new("good"))),
    );
    spec.layers.push(
        LayerSpec::content((0, 0, 8, 8)).with_block(*b"PtFl", ptfl_block(&PtflSpec::new("bad"))),
    );
    spec.extra_global_blocks = tagged_block(*b"Patt", &table);
    let document =
        PsdDocument::from_bytes(build_psd(&spec), &ParseOptions::none()).expect("parses");
    assert_eq!(document.file().patterns.len(), 1);
    assert!(matches!(
        document.file().layers[0].content,
        LayerContent::Pattern(_)
    ));
    assert_eq!(document.file().layers[1].content, LayerContent::Unsupported);
    // A last record that overruns the block also drops without a panic.
    let mut table = pattern_record(&tile4("good"));
    let long = pattern_record(&tile4("long"));
    table.extend_from_slice(&long[..long.len() - 20]);
    spec.extra_global_blocks = tagged_block(*b"Patt", &table);
    let document =
        PsdDocument::from_bytes(build_psd(&spec), &ParseOptions::none()).expect("parses");
    assert_eq!(document.file().patterns.len(), 1);
}

#[test]
fn unsupported_pattern_modes_and_depths_are_dropped() {
    let cmyk = PatternSpec {
        mode: 4,
        planes: vec![vec![0; 4]; 4],
        ..PatternSpec::rgb8("c", 2, 2, &[])
    };
    let deep = PatternSpec {
        depth: 32,
        ..tile4("d")
    };
    let spec = document(
        &[cmyk, deep, tile4("ok")],
        &PtflSpec::new("ok"),
        (0, 0, 8, 8),
    );
    let document =
        PsdDocument::from_bytes(build_psd(&spec), &ParseOptions::none()).expect("parses");
    let ids: Vec<&str> = document
        .file()
        .patterns
        .iter()
        .map(|tile| tile.id.as_str())
        .collect();
    assert_eq!(ids, ["ok"]);
}

/// A 16-bit document that keeps its layer in an `Lr16` block.
fn sixteen_bit(leading: bool, alias: [u8; 4]) -> DocumentSpec {
    let table = pattern_record(&tile4("a"));
    let mut spec = DocumentSpec::new(8, 8, 16, 3);
    spec.layers.push(
        LayerSpec::content((0, 0, 8, 8)).with_block(*b"PtFl", ptfl_block(&PtflSpec::new("a"))),
    );
    spec.layer_info_block = Some(*b"Lr16");
    if leading {
        spec.leading_global_blocks = tagged_block(alias, &table);
    } else {
        spec.extra_global_blocks = tagged_block(alias, &table);
    }
    spec
}

#[test]
fn a_pattern_block_before_or_after_lr16_is_found() {
    for (leading, alias) in [
        (true, *b"Pat2"),
        (false, *b"Pat2"),
        (false, *b"Pat3"),
        (true, *b"Patt"),
    ] {
        let spec = sixteen_bit(leading, alias);
        let region = render(&spec);
        assert_eq!(rgb(&region, 1, 1), tile4_pixel(1, 1), "{alias:?}");
    }
}

#[test]
fn a_psb_file_reads_the_pattern_table_with_four_byte_lengths() {
    let mut spec = document(&[tile4("a")], &PtflSpec::new("a"), (0, 0, 8, 8));
    spec.psb = true;
    let region = render(&spec);
    assert_eq!(rgb(&region, 6, 7), tile4_pixel(6, 7));
    let mut spec = sixteen_bit(false, *b"Pat2");
    spec.psb = true;
    let region = render(&spec);
    assert_eq!(rgb(&region, 6, 7), tile4_pixel(6, 7));
}

#[test]
fn tile_renders_equal_the_full_render() {
    let mut fill = PtflSpec::new("a");
    fill.scale = 150.0;
    fill.angle = 20.0;
    fill.phase = (1.5, 0.25);
    let spec = document(&[tile4("a")], &fill, (1, 1, 8, 8));
    let document =
        PsdDocument::from_bytes(build_psd(&spec), &ParseOptions::none()).expect("parses");
    let options = ParseOptions::none();
    let whole = document
        .composite_region(Viewport::full(8, 8), &options)
        .expect("whole");
    for (left, top) in [(0u32, 0u32), (4, 0), (0, 4), (4, 4)] {
        let part = document
            .composite_region(Viewport::new(left, top, left + 4, top + 4), &options)
            .expect("tile");
        for y in 0..4 {
            for x in 0..4 {
                assert_eq!(
                    sample_rgba8(&part, x, y),
                    sample_rgba8(&whole, left + x, top + y),
                    "tile ({left}, {top}) pixel ({x}, {y})"
                );
            }
        }
    }
}

#[test]
fn a_clean_lr16_file_reports_no_dropped_error_after_a_bad_trailing_block() {
    let mut spec = sixteen_bit(false, *b"Pat2");
    // A block header that claims more data than the section holds.
    let mut bad = b"8BIM".to_vec();
    bad.extend_from_slice(b"junk");
    bad.extend_from_slice(&0xFFFF_FFF0u32.to_be_bytes());
    spec.extra_global_blocks.extend_from_slice(&bad);
    let document =
        PsdDocument::from_bytes(build_psd(&spec), &ParseOptions::none()).expect("parses");
    assert_eq!(document.file().layer_info_error, None);
    assert_eq!(document.file().layers.len(), 1);
    assert!(matches!(
        document.file().layers[0].content,
        LayerContent::Pattern(_)
    ));
}

#[test]
fn an_indexed_palette_without_the_extra_bytes_is_read() {
    let mut palette = vec![0u8; 768];
    palette[3..6].copy_from_slice(&[200, 100, 50]);
    let pattern = PatternSpec {
        mode: 2,
        planes: vec![vec![1, 1, 1, 1]],
        palette: Some(palette),
        tight_palette: true,
        ..PatternSpec::rgb8("i", 2, 2, &[])
    };
    let region = render(&document(&[pattern], &PtflSpec::new("i"), (0, 0, 8, 8)));
    assert_eq!(rgb(&region, 1, 1), [200, 100, 50]);
}

#[test]
fn an_invalid_short_depth_falls_back_to_the_wide_depth() {
    let mut pattern = tile4("a");
    pattern.short_depth = Some(0);
    let region = render(&document(&[pattern], &PtflSpec::new("a"), (0, 0, 8, 8)));
    assert_eq!(rgb(&region, 3, 2), tile4_pixel(3, 2));
}

#[test]
fn a_sixteen_bit_rle_rgb_pattern_with_a_smaller_channel_rect_decodes() {
    let plane = |value: u16| vec![value * 257; 4];
    let pattern = PatternSpec {
        depth: 16,
        rle: true,
        planes: vec![plane(10), plane(20), plane(30)],
        channel_rect: Some((1, 1, 3, 3)),
        ..PatternSpec::rgb8("w", 4, 4, &[])
    };
    let region = render(&document(&[pattern], &PtflSpec::new("w"), (0, 0, 8, 8)));
    assert_eq!(rgb(&region, 1, 1), [10, 20, 30]);
    assert_eq!(rgb(&region, 2, 2), [10, 20, 30]);
    assert_eq!(rgb(&region, 0, 0), [0, 0, 0]);
    assert_eq!(rgb(&region, 3, 3), [0, 0, 0]);
}

/// A table with RGB raw, indexed and grey RLE patterns, framed as a `Patt` block.
fn mixed_table() -> Vec<u8> {
    let mut palette = vec![0u8; 768];
    palette[3..6].copy_from_slice(&[9, 8, 7]);
    let indexed = PatternSpec {
        mode: 2,
        planes: vec![vec![1, 1, 1, 1]],
        palette: Some(palette),
        ..PatternSpec::rgb8("i", 2, 2, &[])
    };
    let mut grey = PatternSpec::grey("g", 4, 4, 16, &[257; 16]);
    grey.rle = true;
    let mut table = Vec::new();
    for pattern in [tile4("a"), indexed, grey] {
        table.extend_from_slice(&pattern_record(&pattern));
    }
    table
}

#[test]
fn a_table_cut_at_every_length_parses_without_a_panic() {
    let table = mixed_table();
    let fill = PtflSpec::new("a");
    for cut in 0..=table.len() {
        let mut spec = document(&[], &fill, (0, 0, 8, 8));
        spec.extra_global_blocks = tagged_block(*b"Patt", &table[..cut]);
        let region = render(&spec);
        let _ = sample_rgba8(&region, 1, 1);
        // The block itself cut short.
        let framed = tagged_block(*b"Patt", &table);
        spec.extra_global_blocks = framed[..cut.min(framed.len())].to_vec();
        let region = render(&spec);
        let _ = sample_rgba8(&region, 1, 1);
    }
}

#[test]
fn a_corrupt_record_length_parses_without_a_panic() {
    let table = mixed_table();
    let fill = PtflSpec::new("a");
    for length in [0u32, 1, 3, 7, 0x100, 0x7FFF_FFFF, 0xFFFF_FFFF] {
        let mut broken = table.clone();
        broken[..4].copy_from_slice(&length.to_be_bytes());
        let mut spec = document(&[], &fill, (0, 0, 8, 8));
        spec.extra_global_blocks = tagged_block(*b"Patt", &broken);
        let region = render(&spec);
        let _ = sample_rgba8(&region, 1, 1);
    }
}

#[test]
fn the_size_caps_drop_a_pattern() {
    // Only a one-pixel channel is written, so the data stays small.
    let huge = |id: &str, width: u32, height: u32| PatternSpec {
        channel_rect: Some((0, 0, 1, 1)),
        ..PatternSpec::grey(id, width, height, 8, &[5])
    };
    let spec = document(
        &[
            huge("edge", 30_001, 1),
            huge("pixels", 4_097, 1_025),
            huge("fits", 4_096, 1_024),
        ],
        &PtflSpec::new("fits"),
        (0, 0, 8, 8),
    );
    let document =
        PsdDocument::from_bytes(build_psd(&spec), &ParseOptions::none()).expect("parses");
    let ids: Vec<&str> = document
        .file()
        .patterns
        .iter()
        .map(|entry| entry.id.as_str())
        .collect();
    assert_eq!(ids, ["fits"]);
}

#[test]
fn patterns_decode_on_first_use_and_within_the_document_budget() {
    // Three 3M pixel patterns: two fit the 8M decoded pixel budget, three do not.
    let big = |id: &str| PatternSpec {
        channel_rect: Some((0, 0, 1, 1)),
        ..PatternSpec::grey(id, 2_000, 1_500, 8, &[200])
    };
    let mut spec = document(
        &[big("a"), big("b"), big("c"), big("unused")],
        &PtflSpec::new("a"),
        (0, 0, 8, 8),
    );
    for id in ["b", "c"] {
        spec.layers.push(
            LayerSpec::content((0, 0, 8, 8)).with_block(*b"PtFl", ptfl_block(&PtflSpec::new(id))),
        );
    }
    let document =
        PsdDocument::from_bytes(build_psd(&spec), &ParseOptions::none()).expect("parses");
    assert!(
        document
            .file()
            .patterns
            .iter()
            .all(|entry| !entry.is_decoded())
    );
    document
        .composite_region(Viewport::full(8, 8), &ParseOptions::none())
        .expect("composites");
    let decoded: Vec<(&str, bool)> = document
        .file()
        .patterns
        .iter()
        .map(|entry| (entry.id.as_str(), entry.is_decoded()))
        .collect();
    assert_eq!(
        decoded,
        [("a", true), ("b", true), ("c", false), ("unused", false)]
    );
}

#[test]
fn four_threads_render_the_same_region_from_one_shared_document() {
    let mut fill = PtflSpec::new("a");
    fill.scale = 150.0;
    let spec = document(&[tile4("a")], &fill, (1, 1, 8, 8));
    let document = std::sync::Arc::new(
        PsdDocument::from_bytes(build_psd(&spec), &ParseOptions::none()).expect("parses"),
    );
    let outputs: Vec<Vec<[u8; 4]>> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..4)
            .map(|_| {
                let document = std::sync::Arc::clone(&document);
                scope.spawn(move || {
                    let region = document
                        .composite_region(Viewport::full(8, 8), &ParseOptions::none())
                        .expect("composites");
                    (0..8)
                        .flat_map(|y| (0..8).map(move |x| (x, y)))
                        .map(|(x, y)| sample_rgba8(&region, x, y))
                        .collect()
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|handle| handle.join().expect("thread"))
            .collect()
    });
    assert!(outputs.iter().all(|output| *output == outputs[0]));
    assert!(document.file().patterns[0].is_decoded());
}
