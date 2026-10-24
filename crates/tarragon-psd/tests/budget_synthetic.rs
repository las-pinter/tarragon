//! The memory budget: effects padding on every layer kind, the merged-preview
//! path, and the tile size that fits.

mod common;

use common::{
    DocumentSpec, LayerSpec, assert_suggested_edge_fits, build_psd, lfx2, peak_resident_bytes,
    push_u16, shadow_effect, stitched_rgba8,
};
use tarragon_psd::limits::MAX_COMPOSITE_BYTES;
use tarragon_psd::{FULL_TIER_MAX_EDGE, ParseOptions, PsdDocument, PsdError, Viewport};

/// A square canvas with one opaque layer, with a drop shadow when asked.
fn one_layer(canvas: u32, shadow: bool) -> PsdDocument {
    let mut spec = DocumentSpec::new(canvas, canvas, 8, 3);
    let edge = i32::try_from(canvas).expect("canvas") - 100;
    let mut layer = LayerSpec::opaque((100, 100, edge, edge), [200, 60, 60]);
    if shadow {
        let effect = shadow_effect(false, [0.0; 3], 120.0, 8.0, 6.0, 0.0, 100.0, true);
        layer = layer.with_block(*b"lfx2", lfx2(&[effect]));
    }
    spec.layers.push(layer);
    let mut section = Vec::new();
    push_u16(&mut section, 1);
    section.extend(std::iter::repeat_n(0u8, 3 * canvas as usize * 2));
    spec.merged_section = Some(section);
    PsdDocument::from_bytes(build_psd(&spec), &ParseOptions::none()).expect("parses")
}

#[test]
fn a_layer_with_effects_and_no_group_is_padded() {
    let plain = one_layer(2100, false);
    let shadowed = one_layer(2100, true);
    assert!(shadowed.max_tile_pixels().unwrap() < plain.max_tile_pixels().unwrap());
    assert!(shadowed.suggested_tile_edge().unwrap() < plain.suggested_tile_edge().unwrap());
    // The plain canvas fits as one tile. The same canvas with a shadow does not.
    let full = Viewport::full(2100, 2100);
    assert!(plain.composite_region(full, &ParseOptions::none()).is_ok());
    let refused = shadowed.composite_region(full, &ParseOptions::none());
    assert!(matches!(refused, Err(PsdError::OverBudget { .. })));
}

#[test]
fn a_shadow_layer_composites_in_suggested_tiles_within_the_budget() {
    let shadowed = one_layer(2100, true);
    let edge = shadowed.suggested_tile_edge().unwrap();
    assert!(edge + 2 * 512 <= FULL_TIER_MAX_EDGE);
    assert!(edge < 2100);
    let pixels = stitched_rgba8(&shadowed, 2100, 2100, edge);
    let at = |x: usize, y: usize| pixels[(y * 2100 + x) * 4];
    // The shadow lands down and right of the layer, over the white backdrop.
    assert_eq!(at(10, 10), 255);
    assert!(at(2100 - 100 + 4, 1200) < 255, "the shadow is drawn");
    assert_suggested_edge_fits(&shadowed, 2100, 512);
    // A smoke check: the peak is process-wide, so it is read only in a fresh
    // process and does not prove the estimate for this tile.
    if let Some(peak) = peak_resident_bytes() {
        assert!(
            peak < MAX_COMPOSITE_BYTES,
            "peak {peak} is within the budget"
        );
    }
}

#[test]
fn a_flattened_document_is_checked_before_the_merged_read() {
    let mut spec = DocumentSpec::new(8192, 8192, 8, 3);
    let mut section = Vec::new();
    push_u16(&mut section, 1);
    section.extend(std::iter::repeat_n(0u8, 3 * 8192 * 2));
    spec.merged_section = Some(section);
    let document = PsdDocument::from_bytes(build_psd(&spec), &ParseOptions::none()).expect("ok");
    let started = std::time::Instant::now();
    let refused = document.composite_region(Viewport::full(8192, 8192), &ParseOptions::none());
    assert!(matches!(refused, Err(PsdError::OverBudget { .. })));
    assert!(started.elapsed() < std::time::Duration::from_secs(1));
    let edge = document.suggested_tile_edge().unwrap();
    let tile = Viewport::new(0, 0, edge, edge);
    assert!(
        document
            .composite_region(tile, &ParseOptions::none())
            .is_ok()
    );
}

/// A canvas with one shadowed layer, wrapped in a shadowed group when asked.
fn shadow_document(canvas: u32, group_effect: bool) -> PsdDocument {
    let mut spec = DocumentSpec::new(canvas, canvas, 8, 3);
    let shadow = || shadow_effect(false, [0.0; 3], 120.0, 8.0, 6.0, 0.0, 100.0, true);
    if group_effect {
        spec.layers.push(LayerSpec::group_marker(3, *b"pass"));
    }
    spec.layers.push(
        LayerSpec::opaque((200, 200, 600, 600), [200, 60, 60])
            .with_block(*b"lfx2", lfx2(&[shadow()])),
    );
    if group_effect {
        spec.layers
            .push(LayerSpec::group_marker(1, *b"pass").with_block(*b"lfx2", lfx2(&[shadow()])));
    }
    let mut section = Vec::new();
    push_u16(&mut section, 1);
    section.extend(std::iter::repeat_n(0u8, 3 * canvas as usize * 2));
    spec.merged_section = Some(section);
    PsdDocument::from_bytes(build_psd(&spec), &ParseOptions::none()).expect("ok")
}

#[test]
fn a_group_effect_around_a_layer_effect_pads_twice() {
    let canvas = 1800u32;
    let document = shadow_document(canvas, true);
    let edge = document.suggested_tile_edge().unwrap();
    // Two levels of padding bound the working size by the canvas here.
    assert_eq!(edge, canvas);
    assert_suggested_edge_fits(&document, canvas, 1024);
    let pixels = stitched_rgba8(&document, canvas, canvas, edge);
    assert_eq!(pixels[(10 * canvas as usize + 10) * 4], 255);
}

#[test]
fn two_padding_levels_are_refused_where_one_level_fits() {
    // At 1900 px both levels pad past the canvas, so the working size is the
    // whole canvas. Two levels cost 312 bytes per pixel and are over the
    // budget; one level costs 272 and fits.
    let canvas = 1900u32;
    let nested = shadow_document(canvas, true);
    assert_eq!(nested.suggested_tile_edge().unwrap(), 1);
    let refused = nested.composite_region(Viewport::new(0, 0, 64, 64), &ParseOptions::none());
    assert!(matches!(refused, Err(PsdError::OverBudget { .. })));

    let single = shadow_document(canvas, false);
    assert_eq!(single.suggested_tile_edge().unwrap(), canvas);
    assert_suggested_edge_fits(&single, canvas, 512);
    let pixels = stitched_rgba8(&single, canvas, canvas, canvas);
    assert_eq!(pixels[(10 * canvas as usize + 10) * 4], 255);
}
