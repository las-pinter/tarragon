//! Resource limits on hostile synthetic files.
//!
//! The files here are small on disk and ask for far more than the file holds:
//! deep group nesting, many channels, and empty channels over a large canvas.
//! Each must return a typed error or a small result, never abort.

mod common;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use common::{DocumentSpec, LayerSpec, build_psd, crop_rgba8, push_u16, stitched_rgba8};
use tarragon_psd::limits::{MAX_GROUP_DEPTH, MAX_HEADER_CHANNELS, MAX_LAYER_CHANNELS};
use tarragon_psd::{ParseOptions, PsdDocument, PsdError, Viewport};

/// The stack size of the worker thread that must survive hostile input.
const SMALL_STACK: usize = 256 * 1024;

/// Run `work` on a thread with a small stack. A stack overflow aborts the whole
/// test process, so a passing run proves the recursion is bounded.
fn on_small_stack<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> T {
    std::thread::Builder::new()
        .stack_size(SMALL_STACK)
        .spawn(work)
        .expect("the thread starts")
        .join()
        .expect("the thread finishes")
}

/// A 16x16 file with `depth` nested groups around one pixel layer.
fn nested_groups(depth: usize) -> Vec<u8> {
    let mut spec = DocumentSpec::new(16, 16, 8, 3);
    for _ in 0..depth {
        spec.layers.push(LayerSpec::group_marker(3, *b"pass"));
    }
    spec.layers
        .push(LayerSpec::opaque((0, 0, 16, 16), [10, 20, 30]));
    for _ in 0..depth {
        spec.layers.push(LayerSpec::group_marker(1, *b"pass"));
    }
    build_psd(&spec)
}

#[test]
fn thousands_of_nested_groups_are_an_error_on_a_small_stack() {
    let bytes = nested_groups(2000);
    let result = on_small_stack(move || {
        let document = PsdDocument::from_bytes(bytes, &ParseOptions::none())?;
        document.composite_region(Viewport::full(16, 16), &ParseOptions::none())
    });
    assert!(matches!(result, Err(PsdError::GroupsTooDeep { .. })));
}

#[test]
fn groups_at_the_depth_cap_composite_on_a_small_stack() {
    let bytes = nested_groups(MAX_GROUP_DEPTH);
    let pixel = on_small_stack(move || {
        let document = PsdDocument::from_bytes(bytes, &ParseOptions::none()).expect("parses");
        let region = document
            .composite_region(Viewport::full(16, 16), &ParseOptions::none())
            .expect("composites");
        region.rgba8()[..4].to_vec()
    });
    assert_eq!(pixel, [10, 20, 30, 255]);
}

#[test]
fn deeply_nested_groups_over_a_large_tile_hit_the_memory_budget() {
    let mut spec = DocumentSpec::new(4096, 4096, 8, 3);
    for _ in 0..MAX_GROUP_DEPTH {
        spec.layers.push(LayerSpec::group_marker(3, *b"pass"));
    }
    spec.layers
        .push(LayerSpec::opaque((0, 0, 16, 16), [10, 20, 30]));
    for _ in 0..MAX_GROUP_DEPTH {
        spec.layers.push(LayerSpec::group_marker(1, *b"pass"));
    }
    spec.merged_section = Some(empty_rle_section(3, 4096));
    let document = PsdDocument::from_bytes(build_psd(&spec), &ParseOptions::none()).expect("ok");
    let started = std::time::Instant::now();
    let result = document.composite_region(Viewport::full(4096, 4096), &ParseOptions::none());
    assert!(matches!(result, Err(PsdError::OverBudget { .. })));
    assert!(started.elapsed() < std::time::Duration::from_secs(2));
}

/// A canvas with `depth` nested groups around two layers that cross tile edges.
fn canvas_with_groups(width: u32, height: u32, depth: usize) -> DocumentSpec {
    let mut spec = DocumentSpec::new(width, height, 8, 3);
    let w = i32::try_from(width).expect("width");
    let h = i32::try_from(height).expect("height");
    for _ in 0..depth {
        spec.layers.push(LayerSpec::group_marker(3, *b"pass"));
    }
    spec.layers
        .push(LayerSpec::opaque((10, 10, h - 10, w - 10), [200, 40, 40]));
    let mut second = LayerSpec::opaque((h / 3, w / 3, h, w), [20, 60, 220]);
    second.opacity = 128;
    spec.layers.push(second);
    for _ in 0..depth {
        spec.layers.push(LayerSpec::group_marker(1, *b"pass"));
    }
    spec.merged_section = Some(empty_rle_section(3, height as usize));
    spec
}

#[test]
fn a_refused_canvas_composites_in_tiles_of_the_suggested_size() {
    // Depth 12 costs 576 bytes per pixel, so 2.1 million pixels is over 1 GiB.
    let (width, height) = (1500, 1400);
    let bytes = build_psd(&canvas_with_groups(width, height, 12));
    let document = PsdDocument::from_bytes(bytes, &ParseOptions::none()).expect("parses");
    let refused = document.composite_region(Viewport::full(width, height), &ParseOptions::none());
    let Err(PsdError::OverBudget {
        max_tile_pixels, ..
    }) = refused
    else {
        panic!("one tile over the budget must be refused");
    };
    assert_eq!(document.max_tile_pixels().expect("tree"), max_tile_pixels);
    let edge = document.suggested_tile_edge().expect("tree");
    assert!(u64::from(edge) * u64::from(edge) <= max_tile_pixels);
    assert!(edge < width, "the suggestion splits the canvas");
    let tiled = stitched_rgba8(&document, width, height, edge);
    // A small viewport composited alone must match the stitched pixels, at the
    // tile seams and away from them.
    for view in [
        Viewport::new(edge - 20, edge - 20, edge + 20, edge + 20),
        Viewport::new(0, 0, 64, 64),
        Viewport::new(width - 64, height - 64, width, height),
        Viewport::new(edge - 30, 400, edge + 30, 460),
    ] {
        let alone = document
            .composite_region(view, &ParseOptions::none())
            .expect("a small view fits");
        assert!(alone.rgba8() == crop_rgba8(&tiled, width, view), "{view:?}");
    }
    let at = |x: usize, y: usize| {
        let i = (y * width as usize + x) * 4;
        [tiled[i], tiled[i + 1], tiled[i + 2], tiled[i + 3]]
    };
    assert_eq!(at(5, 5), [255, 255, 255, 0]);
    assert_eq!(at(100, 100), [200, 40, 40, 255]);
    assert_ne!(at(1400, 1300), at(100, 100));
}

#[test]
fn nested_groups_under_the_budget_still_composite() {
    let document = PsdDocument::from_bytes(nested_groups(MAX_GROUP_DEPTH), &ParseOptions::none())
        .expect("parses");
    let region = document
        .composite_region(Viewport::full(16, 16), &ParseOptions::none())
        .expect("composites");
    assert_eq!(region.rgba8()[..4], [10, 20, 30, 255]);
}

#[test]
fn a_layer_with_too_many_channels_is_a_typed_error() {
    let mut layer = LayerSpec::content((0, 0, 0, 0));
    layer.channels = (0..100).map(|id| (id, Vec::new())).collect();
    let mut spec = DocumentSpec::new(16, 16, 8, 3);
    spec.layers.push(layer);
    let result = PsdDocument::from_bytes(build_psd(&spec), &ParseOptions::none());
    assert!(matches!(
        result,
        Err(PsdError::TooManyChannels {
            count: 100,
            limit: MAX_LAYER_CHANNELS
        })
    ));
}

#[test]
fn a_header_with_too_many_channels_is_a_typed_error() {
    let mut spec = DocumentSpec::new(16, 16, 8, 3);
    spec.merged_channels = Some(u16::try_from(MAX_HEADER_CHANNELS + 1).expect("fits"));
    let result = PsdDocument::from_bytes(build_psd(&spec), &ParseOptions::none());
    assert!(matches!(result, Err(PsdError::TooManyChannels { .. })));
}

/// An RLE merged section whose row lengths are all zero.
fn empty_rle_section(channels: usize, rows: usize) -> Vec<u8> {
    let mut section = Vec::new();
    push_u16(&mut section, 1);
    section.extend(std::iter::repeat_n(0u8, channels * rows * 2));
    section
}

#[test]
fn empty_layer_channels_over_a_large_canvas_decode_to_nothing() {
    let mut layer = LayerSpec::content((0, 0, 0, 0));
    // The channels the compositor reads, a repeat of one of them, and many
    // channels it never reads. All of them are empty.
    let ids: Vec<i16> = [0, 1, 2, -1, -2, 0]
        .into_iter()
        .chain(3..60)
        .collect::<Vec<_>>();
    layer.channels = ids.into_iter().map(|id| (id, Vec::new())).collect();
    let mut spec = DocumentSpec::new(8192, 8192, 8, 3);
    spec.layers.push(layer);
    spec.merged_section = Some(empty_rle_section(3, 8192));
    let document = PsdDocument::from_bytes(build_psd(&spec), &ParseOptions::none()).expect("ok");
    let region = document
        .read_layer_region(0, Viewport::full(8192, 8192), &ParseOptions::none())
        .expect("the region reads");
    let ids: Vec<i16> = region.channels.iter().map(|channel| channel.id).collect();
    assert_eq!(ids, [0, 1, 2, -1, -2]);
    let allocated: usize = region
        .channels
        .iter()
        .map(|channel| channel.samples.capacity())
        .sum();
    assert_eq!(allocated, 0, "an empty channel allocates no samples");
    assert_eq!(region.sample(0, 4000, 4000), Some(0));
}

#[test]
fn empty_merged_channels_over_a_large_canvas_decode_to_nothing() {
    let mut spec = DocumentSpec::new(8192, 8192, 8, 3);
    spec.merged_channels = Some(u16::try_from(MAX_HEADER_CHANNELS).expect("fits"));
    spec.merged_section = Some(empty_rle_section(MAX_HEADER_CHANNELS, 8192));
    let document = PsdDocument::from_bytes(build_psd(&spec), &ParseOptions::none()).expect("ok");
    let region = document
        .read_merged_region(Viewport::full(8192, 8192), &ParseOptions::none())
        .expect("the region reads");
    // Only the three color channels are decoded, and they hold no samples.
    assert_eq!(region.channels.len(), 3);
    assert!(region.channels.iter().all(|c| c.samples.capacity() == 0));
}

#[test]
fn a_set_cancel_flag_stops_the_parse_and_the_render() {
    let mut spec = DocumentSpec::new(4, 4, 8, 3);
    spec.layers.push(LayerSpec::opaque((0, 0, 4, 4), [1, 2, 3]));
    let bytes = build_psd(&spec);
    let flag = Arc::new(AtomicBool::new(false));
    let options = ParseOptions::none().with_cancel(Arc::clone(&flag));
    let document = PsdDocument::from_bytes(bytes.clone(), &options).expect("not cancelled yet");
    flag.store(true, Ordering::Relaxed);
    let render = document.composite_region(Viewport::full(4, 4), &options);
    assert!(matches!(render, Err(PsdError::Cancelled)));
    let parse = PsdDocument::from_bytes(bytes, &options);
    assert!(matches!(parse, Err(PsdError::Cancelled)));
}

#[test]
fn a_timeout_is_its_own_transient_error() {
    let mut spec = DocumentSpec::new(4, 4, 8, 3);
    spec.layers.push(LayerSpec::opaque((0, 0, 4, 4), [1, 2, 3]));
    let document = PsdDocument::from_bytes(build_psd(&spec), &ParseOptions::none()).expect("ok");
    let expired = ParseOptions::in_seconds(0);
    let error = document
        .composite_region(Viewport::full(4, 4), &expired)
        .expect_err("the deadline passed");
    assert!(matches!(error, PsdError::Timeout));
    assert!(error.is_transient());
}

#[test]
fn a_gradient_fill_honors_the_deadline() {
    let mut spec = DocumentSpec::new(16, 1, 8, 3);
    spec.layers.push(
        LayerSpec::content((0, 0, 1, 16)).with_block(*b"GdFl", common::gdfl_black_to_white(0.0)),
    );
    let document = PsdDocument::from_bytes(build_psd(&spec), &ParseOptions::none()).expect("ok");
    let result = document.composite_region(Viewport::full(16, 1), &ParseOptions::in_seconds(0));
    assert!(matches!(result, Err(PsdError::Timeout)));
}

#[test]
fn flipping_header_and_record_bytes_never_panics_or_overflows() {
    let mut spec = DocumentSpec::new(8, 8, 8, 3);
    spec.layers.push(LayerSpec::group_marker(3, *b"pass"));
    spec.layers
        .push(LayerSpec::with_alpha((0, 0, 8, 8), [9, 8, 7, 255]).with_mask((0, 0, 4, 4), 255));
    spec.layers.push(LayerSpec::group_marker(1, *b"pass"));
    let original = build_psd(&spec);
    // The header, the section lengths, the layer count, and the first records.
    for at in 0..original.len().min(260) {
        for value in [0x00, 0x01, 0x7F, 0x80, 0xFF] {
            let mut bytes = original.clone();
            bytes[at] = value;
            let options = ParseOptions::in_seconds(10);
            let Ok(document) = PsdDocument::from_bytes(bytes, &options) else {
                continue;
            };
            let header = document.file().header;
            if header.width <= 64 && header.height <= 64 {
                let viewport = Viewport::full(header.width, header.height);
                let _ = document.composite_region(viewport, &options);
            }
        }
    }
}
