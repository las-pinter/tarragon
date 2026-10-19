//! Layer-effects application on synthetic PSD files.
//!
//! Each test pins one paint rule: where a bevel or a stroke lands, how much
//! coverage an overlay takes at an anti-aliased edge, the paint order inside
//! the layer, the gradient stroke frame, the layer opacity, and the tile
//! padding cap on a wide tile.
//!
//! One file covers the paint rules of one compositor path, so the tests stay
//! together.

mod common;

use common::sample_rgba8;
use common::{
    DocumentSpec, LayerSpec, bevel_effect, build_psd, composite, gradient_overlay_effect,
    gradient_stroke_effect, lfx2, set_enum_item, shadow_effect, solid_overlay_effect,
    square_document_with_opacity, stroke_effect,
};
use tarragon_psd::limits::FULL_TIER_MAX_EDGE;
use tarragon_psd::{ParseOptions, PsdDocument, Viewport};

/// A bevel-and-emboss effect with a chosen style.
fn styled_bevel(style: [u8; 4], size: f64, depth: f64) -> ([u8; 4], Vec<u8>) {
    let mut bevel = bevel_effect(size, depth, 120.0, 30.0, [255.0; 3], [0.0; 3], 75.0);
    set_enum_item(&mut bevel.1, b"bvlS", style);
    bevel
}

/// A layer whose first pixel column is partly transparent.
fn edge_alpha_layer(rect: (i32, i32, i32, i32), color: [u16; 3], edge: u16) -> LayerSpec {
    let (top, left, bottom, right) = rect;
    let width = usize::try_from(right - left).expect("width");
    let height = usize::try_from(bottom - top).expect("height");
    let mut alpha = vec![255u16; width * height];
    for y in 0..height {
        alpha[y * width] = edge;
    }
    let mut layer = LayerSpec::with_alpha(rect, [color[0], color[1], color[2], 255]);
    layer.channels = vec![
        (0, vec![color[0]; width * height]),
        (1, vec![color[1]; width * height]),
        (2, vec![color[2]; width * height]),
        (-1, alpha),
    ];
    layer
}

#[test]
fn each_bevel_style_shades_its_own_side_of_the_edge() {
    let cases: [([u8; 4], bool, bool); 3] = [
        (*b"In  ", true, false),
        (*b"OtrB", false, true),
        (*b"Embs", true, true),
    ];
    for (style, inside_expected, outside_expected) in cases {
        let bevel = styled_bevel(style, 6.0, 100.0);
        let bytes = square_document_with_opacity(*b"lfx2", lfx2(&[bevel]), [128, 128, 128], 255);
        let region = composite(bytes, Viewport::full(64, 64));
        let mut inside = false;
        let mut outside = false;
        for y in 16..48u32 {
            for x in 16..48u32 {
                let covered = (24..40).contains(&x) && (24..40).contains(&y);
                let base = if covered { 128 } else { 200 };
                if sample_rgba8(&region, x, y)[0] == base {
                    continue;
                }
                if covered {
                    inside = true;
                } else {
                    outside = true;
                }
            }
        }
        assert_eq!(
            inside, inside_expected,
            "style {style:?} paints inside the shape"
        );
        assert_eq!(
            outside, outside_expected,
            "style {style:?} paints outside the shape"
        );
    }
}

#[test]
fn an_outer_bevel_shades_the_plane_outside_the_shape() {
    // The outer bevel builds its coverage outside the shape. The paint must
    // reach the exterior plane, not be cut by the shape factor.
    let bevel = styled_bevel(*b"OtrB", 6.0, 100.0);
    let bytes = square_document_with_opacity(*b"lfx2", lfx2(&[bevel]), [128, 128, 128], 255);
    let region = composite(bytes, Viewport::full(64, 64));
    let mut outside = false;
    for y in 16..48u32 {
        for x in 16..48u32 {
            let inside = (24..40).contains(&x) && (24..40).contains(&y);
            if !inside && sample_rgba8(&region, x, y)[0] != 200 {
                outside = true;
            }
        }
    }
    assert!(
        outside,
        "the outer bevel shades the plane outside the square"
    );
    assert_eq!(
        sample_rgba8(&region, 32, 32),
        [128, 128, 128, 255],
        "the outer bevel does not paint inside the square"
    );
}

#[test]
fn an_emboss_bevel_shades_both_sides_of_the_edge() {
    let bevel = styled_bevel(*b"Embs", 6.0, 100.0);
    let bytes = square_document_with_opacity(*b"lfx2", lfx2(&[bevel]), [128, 128, 128], 255);
    let region = composite(bytes, Viewport::full(64, 64));
    let mut inside = false;
    let mut outside = false;
    for y in 16..48u32 {
        for x in 16..48u32 {
            let covered = (24..40).contains(&x) && (24..40).contains(&y);
            let base = if covered { 128 } else { 200 };
            if sample_rgba8(&region, x, y)[0] == base {
                continue;
            }
            if covered {
                inside = true;
            } else {
                outside = true;
            }
        }
    }
    assert!(inside, "the emboss shades the layer inside the edge");
    assert!(outside, "the emboss shades the plane outside the edge");
}

#[test]
fn an_overlay_applies_the_edge_coverage_once() {
    // A 50 % edge pixel under a 100 % red overlay keeps its alpha and takes
    // the overlay color: red = 0.5 * 255 + 0.5 * 200, green and blue = 100.
    // A shape squared edge would blend half as much and stay near 131.
    let overlay = solid_overlay_effect([255.0, 0.0, 0.0], 100.0);
    let mut spec = DocumentSpec::new(32, 32, 8, 3);
    spec.layers
        .push(LayerSpec::opaque((0, 0, 32, 32), [200, 200, 200]));
    spec.layers.push(
        edge_alpha_layer((10, 10, 20, 20), [0, 0, 0], 128).with_block(*b"lfx2", lfx2(&[overlay])),
    );
    let region = composite(build_psd(&spec), Viewport::full(32, 32));
    let edge = sample_rgba8(&region, 10, 15);
    assert!(
        (224..=230).contains(&edge[0]),
        "the half-covered edge takes the overlay color: {edge:?}"
    );
    assert!(
        (96..=102).contains(&edge[1]),
        "the half-covered edge keeps its alpha: {edge:?}"
    );
    let center = sample_rgba8(&region, 15, 15);
    assert_eq!(center, [255, 0, 0, 255], "the opaque center is the overlay");
}

#[test]
fn a_clipped_layer_effect_stops_at_the_base_shape() {
    // The clip layer shares the base's left edge. Its 4 px outside stroke
    // would reach x 12..16, outside the base, and must not show there.
    let stroke = stroke_effect([255.0; 3], 4.0, false, 100.0);
    let mut spec = DocumentSpec::new(64, 64, 8, 3);
    spec.layers
        .push(LayerSpec::opaque((0, 0, 64, 64), [200, 200, 200]));
    spec.layers
        .push(LayerSpec::opaque((16, 16, 48, 48), [120, 120, 120]));
    let mut clip = LayerSpec::with_alpha((16, 16, 40, 40), [60, 60, 200, 255])
        .with_block(*b"lfx2", lfx2(&[stroke]));
    clip.clipping = 2;
    spec.layers.push(clip);
    let region = composite(build_psd(&spec), Viewport::full(64, 64));
    assert_eq!(
        sample_rgba8(&region, 13, 28),
        [200, 200, 200, 255],
        "the clipped stroke does not show outside the base"
    );
    let inside = sample_rgba8(&region, 41, 28);
    assert!(
        inside[0] > 200,
        "the clipped stroke shows inside the base: {inside:?}"
    );
}

#[test]
fn a_color_overlay_paints_over_a_gradient_overlay() {
    // The parser's canonical KINDS order puts SoFi above GrFl, so the color
    // overlay is the top item. The gradient overlay is painted before it.
    let gradient = gradient_overlay_effect(0.0, 100.0);
    let color = solid_overlay_effect([255.0, 0.0, 0.0], 100.0);
    let bytes =
        square_document_with_opacity(*b"lfx2", lfx2(&[color, gradient]), [100, 100, 100], 255);
    let region = composite(bytes, Viewport::full(64, 64));
    assert_eq!(sample_rgba8(&region, 25, 32), [255, 0, 0, 255]);
    assert_eq!(sample_rgba8(&region, 39, 32), [255, 0, 0, 255]);
}

#[test]
fn an_inner_bevel_paints_over_an_inside_stroke() {
    let stroke = stroke_effect([255.0; 3], 6.0, true, 100.0);
    let bevel = styled_bevel(*b"In  ", 6.0, 100.0);
    let stroke_only = composite(
        square_document_with_opacity(
            *b"lfx2",
            lfx2(std::slice::from_ref(&stroke)),
            [128, 128, 128],
            255,
        ),
        Viewport::full(64, 64),
    );
    let both = composite(
        square_document_with_opacity(*b"lfx2", lfx2(&[stroke, bevel]), [128, 128, 128], 255),
        Viewport::full(64, 64),
    );
    let mut differs = false;
    for y in 24..40u32 {
        for x in 24..40u32 {
            let band = !(30..34).contains(&x) || !(30..34).contains(&y);
            if band && sample_rgba8(&both, x, y) != sample_rgba8(&stroke_only, x, y) {
                differs = true;
            }
        }
    }
    assert!(differs, "the inner bevel is painted over the inside stroke");
}

#[test]
fn a_gradient_stroke_spans_the_stroke_extent() {
    // An 8 px outside stroke around a 16 px square covers y 16..48. A vertical
    // black-to-white gradient spans that extent, so the first sampled band row
    // is near white and the last is near black. Over the layer bounds the ramp
    // would clamp to white and black.
    let stroke = gradient_stroke_effect(90.0, 8.0, false, 100.0);
    let bytes = square_document_with_opacity(*b"lfx2", lfx2(&[stroke]), [128, 128, 128], 255);
    let region = composite(bytes, Viewport::full(64, 64));
    let top = sample_rgba8(&region, 32, 20)[0];
    let bottom = sample_rgba8(&region, 32, 44)[0];
    assert!(
        (220..=240).contains(&top),
        "the top band sits on the ramp: {top}"
    );
    assert!(
        (18..=40).contains(&bottom),
        "the bottom band sits on the ramp: {bottom}"
    );
}

#[test]
fn the_layer_opacity_applies_to_the_whole_stack() {
    let shadow = shadow_effect(false, [0.0; 3], 120.0, 6.0, 4.0, 0.0, 100.0, true);
    let opaque = composite(
        square_document_with_opacity(
            *b"lfx2",
            lfx2(std::slice::from_ref(&shadow)),
            [100, 100, 100],
            255,
        ),
        Viewport::full(64, 64),
    );
    let faded = composite(
        square_document_with_opacity(*b"lfx2", lfx2(&[shadow]), [100, 100, 100], 128),
        Viewport::full(64, 64),
    );
    // The opaque layer hides the knocked-out shadow, then one 50 % blend mixes
    // the whole stack: 0.5 * 100 + 0.5 * 200.
    let center = sample_rgba8(&faded, 32, 32)[0];
    assert!(
        (147..=152).contains(&center),
        "the stack fades once at the layer opacity: {center}"
    );
    // The exterior shadow fades with the same opacity.
    let shadowed = sample_rgba8(&faded, 42, 45)[0];
    let reference = sample_rgba8(&opaque, 42, 45)[0];
    assert!(
        shadowed > reference,
        "the faded shadow {shadowed} is lighter than {reference}"
    );
}

#[test]
fn a_tile_at_the_cap_width_keeps_its_vertical_padding() {
    // The padding cap is per axis. An 8192 px wide tile has no room across,
    // but it still pads along y, so a shadow that crosses the tile row does
    // not seam.
    let width = FULL_TIER_MAX_EDGE;
    let mut spec = DocumentSpec::new(width, 32, 8, 3);
    spec.layers
        .push(LayerSpec::opaque((0, 0, 32, 8192), [200, 200, 200]));
    let shadow = shadow_effect(false, [0.0; 3], 90.0, 3.0, 8.0, 0.0, 80.0, false);
    let bar =
        LayerSpec::opaque((8, 4096, 16, 4200), [60, 60, 60]).with_block(*b"lfx2", lfx2(&[shadow]));
    spec.layers.push(bar);
    let document = PsdDocument::from_bytes(build_psd(&spec), &ParseOptions::none())
        .expect("the wide file parses");
    let full = document
        .composite_region(Viewport::new(0, 0, width, 32), &ParseOptions::none())
        .expect("the full composite");
    let lower = document
        .composite_region(Viewport::new(0, 16, width, 32), &ParseOptions::none())
        .expect("the lower tile");
    for x in 4090..4210u32 {
        assert_eq!(
            sample_rgba8(&lower, x, 0),
            sample_rgba8(&full, x, 16),
            "the tile seam returns at x {x}"
        );
    }
}
