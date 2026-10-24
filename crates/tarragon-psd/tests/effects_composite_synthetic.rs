//! Layer-effects compositing on synthetic PSD files.
//!
//! The tests read exact pixels from the compositor. Every expectation follows
//! the effect rule that the test names.

mod common;

use common::sample_rgba8;
use common::{
    DocumentSpec, LayerSpec, build_psd, composite, glow_effect, gradient_overlay_effect,
    legacy_lrfx, legacy_shadow_body_with_light, lfx2, satin_effect, set_bool_item, shadow_effect,
    solid_overlay_effect, square_document_with_fill_opacity, stroke_effect,
};
use tarragon_psd::{Effect, ParseOptions, PsdDocument, Viewport};

#[test]
fn a_drop_shadow_darkens_the_backdrop_below_and_right() {
    let shadow = shadow_effect(false, [0.0; 3], 120.0, 8.0, 6.0, 0.0, 75.0, true);
    let bytes = square_document_with_fill_opacity(*b"lfx2", lfx2(&[shadow]), [200, 60, 60], None);
    let region = composite(bytes, Viewport::full(64, 64));
    // Far from the square the backdrop is unchanged.
    assert_eq!(sample_rgba8(&region, 5, 5), [200, 200, 200, 255]);
    // The shadow lands down and right of the square.
    let lit = sample_rgba8(&region, 5, 5)[0];
    let shadowed = sample_rgba8(&region, 42, 45)[0];
    assert!(shadowed < lit, "shadow {shadowed} is darker than {lit}");
}

#[test]
fn the_document_global_angle_turns_a_shadow_that_follows_it() {
    // The document light comes from the left; the effect asks for 120 degrees.
    let mut shadow = shadow_effect(false, [0.0; 3], 120.0, 8.0, 6.0, 0.0, 100.0, true);
    set_bool_item(&mut shadow.1, b"uglg", true);
    let mut spec = DocumentSpec::new(64, 64, 8, 3);
    spec.resources.push((1037, 0i32.to_be_bytes().to_vec()));
    spec.layers
        .push(LayerSpec::opaque((0, 0, 64, 64), [200, 200, 200]));
    spec.layers.push(
        LayerSpec::with_alpha((24, 24, 40, 40), [200, 60, 60, 255])
            .with_block(*b"lfx2", lfx2(&[shadow])),
    );
    let region = composite(build_psd(&spec), Viewport::full(64, 64));
    assert_eq!(sample_rgba8(&region, 5, 5), [200, 200, 200, 255]);
    let left = sample_rgba8(&region, 18, 30)[0];
    assert!(left < 200, "the global light puts the shadow at {left}");
    assert_eq!(
        sample_rgba8(&region, 44, 30),
        [200, 200, 200, 255],
        "the effect angle does not move the shadow to the right"
    );
}

#[test]
fn the_document_global_angle_turns_a_legacy_shadow_that_follows_it() {
    // The document light comes from the left; the effect asks for 120 degrees.
    let body = legacy_shadow_body_with_light(120.0, 8.0, 6.0, 255, true, true);
    let legacy = legacy_lrfx(&[(*b"dsdw", body)]);
    let mut spec = DocumentSpec::new(64, 64, 8, 3);
    spec.resources.push((1037, 0i32.to_be_bytes().to_vec()));
    spec.layers
        .push(LayerSpec::opaque((0, 0, 64, 64), [200, 200, 200]));
    spec.layers.push(
        LayerSpec::with_alpha((24, 24, 40, 40), [200, 60, 60, 255]).with_block(*b"lrFX", legacy),
    );
    let region = composite(build_psd(&spec), Viewport::full(64, 64));
    let left = sample_rgba8(&region, 18, 32)[0];
    assert!(left < 200, "the global light puts the shadow at {left}");
    assert_eq!(
        sample_rgba8(&region, 46, 32),
        [200, 200, 200, 255],
        "the effect angle does not move the shadow to the right"
    );
}

#[test]
fn a_bevel_that_follows_the_global_light_uses_the_document_angle() {
    // The effect asks for 120 degrees; the document light comes from 0.
    let edge = |use_global: bool| {
        let mut bevel = common::bevel_effect(6.0, 1.0, 120.0, 30.0, [255.0; 3], [0.0; 3], 100.0);
        set_bool_item(&mut bevel.1, b"uglg", use_global);
        let mut spec = DocumentSpec::new(64, 64, 8, 3);
        spec.resources.push((1037, 0i32.to_be_bytes().to_vec()));
        spec.layers
            .push(LayerSpec::opaque((0, 0, 64, 64), [200, 200, 200]));
        spec.layers.push(
            LayerSpec::with_alpha((24, 24, 40, 40), [128, 128, 128, 255])
                .with_block(*b"lfx2", lfx2(&[bevel])),
        );
        let region = composite(build_psd(&spec), Viewport::full(64, 64));
        (
            sample_rgba8(&region, 25, 32)[0],
            sample_rgba8(&region, 38, 32)[0],
        )
    };
    let (global_left, global_right) = edge(true);
    assert!(
        global_left > global_right,
        "the document light highlights the left edge: left {global_left} right {global_right}"
    );
    let (own_left, own_right) = edge(false);
    assert!(
        own_right > own_left,
        "the effect angle highlights the right edge: left {own_left} right {own_right}"
    );
}

#[test]
fn a_gradient_on_a_group_spans_the_group_bounds() {
    // A group close record has a zero rectangle. The frame must come from the
    // child layers, or the ramp collapses to its end colour.
    let overlay = gradient_overlay_effect(0.0, 100.0);
    let mut spec = DocumentSpec::new(64, 64, 8, 3);
    spec.layers
        .push(LayerSpec::opaque((0, 0, 64, 64), [200, 200, 200]));
    spec.layers.push(LayerSpec::group_marker(3, *b"pass"));
    spec.layers.push(LayerSpec::with_alpha(
        (24, 24, 40, 40),
        [128, 128, 128, 255],
    ));
    let close = LayerSpec::group_marker(1, *b"pass").with_block(*b"lfx2", lfx2(&[overlay]));
    spec.layers.push(close);
    let region = composite(build_psd(&spec), Viewport::full(64, 64));
    let left = sample_rgba8(&region, 25, 32)[0];
    let right = sample_rgba8(&region, 39, 32)[0];
    assert!(left < right, "left {left} is darker than right {right}");
    assert!(left < 128, "the left edge takes the dark end: {left}");
}

#[test]
fn an_outer_glow_lightens_the_backdrop_outside_the_square() {
    let glow = glow_effect(false, false, [255.0, 255.0, 255.0], 10.0, 0.0, 0.5, 75.0);
    let bytes = square_document_with_fill_opacity(*b"lfx2", lfx2(&[glow]), [60, 60, 60], None);
    let region = composite(bytes, Viewport::full(64, 64));
    let outside = sample_rgba8(&region, 20, 32)[0];
    let far = sample_rgba8(&region, 2, 2)[0];
    assert!(outside > far, "glow {outside} is lighter than {far}");
}

#[test]
fn an_inner_shadow_darkens_the_layer_inside_its_edge() {
    let shadow = shadow_effect(true, [0.0; 3], 120.0, 6.0, 4.0, 0.0, 75.0, true);
    let bytes = square_document_with_fill_opacity(*b"lfx2", lfx2(&[shadow]), [120, 120, 120], None);
    let region = composite(bytes, Viewport::full(64, 64));
    let edge = sample_rgba8(&region, 25, 25)[0];
    let center = sample_rgba8(&region, 32, 32)[0];
    assert!(edge < center, "inner edge {edge} is darker than {center}");
}

#[test]
fn an_inner_stroke_paints_the_inside_band_only() {
    let stroke = stroke_effect([255.0, 255.0, 255.0], 4.0, true, 100.0);
    let bytes = square_document_with_fill_opacity(*b"lfx2", lfx2(&[stroke]), [100, 100, 100], None);
    let region = composite(bytes, Viewport::full(64, 64));
    let band = sample_rgba8(&region, 25, 32);
    assert!(band[0] > 200, "inside stroke {band:?} is light");
    assert_eq!(sample_rgba8(&region, 32, 32), [100, 100, 100, 255]);
}

#[test]
fn a_color_overlay_replaces_the_layer_color() {
    let overlay = solid_overlay_effect([255.0, 0.0, 0.0], 100.0);
    let bytes =
        square_document_with_fill_opacity(*b"lfx2", lfx2(&[overlay]), [100, 100, 100], None);
    let region = composite(bytes, Viewport::full(64, 64));
    assert_eq!(sample_rgba8(&region, 32, 32), [255, 0, 0, 255]);
    // The backdrop outside the square is untouched.
    assert_eq!(sample_rgba8(&region, 5, 5), [200, 200, 200, 255]);
}

#[test]
fn a_gradient_overlay_darkens_the_left_side() {
    let overlay = gradient_overlay_effect(0.0, 100.0);
    let bytes =
        square_document_with_fill_opacity(*b"lfx2", lfx2(&[overlay]), [128, 128, 128], None);
    let region = composite(bytes, Viewport::full(64, 64));
    let left = sample_rgba8(&region, 25, 32)[0];
    let right = sample_rgba8(&region, 39, 32)[0];
    assert!(left < right, "left {left} is darker than right {right}");
}

#[test]
fn the_fill_opacity_lets_an_exterior_shadow_show_through() {
    let shadow = shadow_effect(false, [0.0; 3], 120.0, 6.0, 4.0, 0.0, 100.0, true);
    let opaque = composite(
        square_document_with_fill_opacity(
            *b"lfx2",
            lfx2(std::slice::from_ref(&shadow)),
            [120, 120, 120],
            None,
        ),
        Viewport::full(64, 64),
    );
    let transparent = composite(
        square_document_with_fill_opacity(*b"lfx2", lfx2(&[shadow]), [120, 120, 120], Some(64)),
        Viewport::full(64, 64),
    );
    let opaque_center = sample_rgba8(&opaque, 32, 32)[0];
    let transparent_center = sample_rgba8(&transparent, 32, 32)[0];
    // Without a shadow, fill 25 % would blend to 0.25 * 120 + 0.75 * 200 = 180.
    assert!(
        transparent_center > opaque_center && transparent_center < 180,
        "fill 25 % center {transparent_center} shows the shadow under {opaque_center}"
    );
}

#[test]
fn an_unsupported_paint_makes_the_stroke_a_no_op() {
    // A pattern stroke has no pattern table, so the effect paints nothing.
    let mut stroke = stroke_effect([255.0, 255.0, 255.0], 4.0, true, 100.0);
    // Replace the paint type enum item with a pattern type. The helper writes
    // `PntT` before the color; patch the four-character value in place.
    let position = find(&stroke.1, *b"SClr").expect("paint type");
    stroke.1[position..position + 4].copy_from_slice(b"Ptrn");
    let bytes = square_document_with_fill_opacity(*b"lfx2", lfx2(&[stroke]), [100, 100, 100], None);
    let region = composite(bytes, Viewport::full(64, 64));
    assert_eq!(sample_rgba8(&region, 25, 32), [100, 100, 100, 255]);
}

/// Find a four-byte code in a byte slice.
fn find(data: &[u8], code: [u8; 4]) -> Option<usize> {
    data.windows(4).position(|window| window == code)
}

#[test]
fn a_bevel_and_a_satin_change_the_layer_edges() {
    let bevel = common::bevel_effect(8.0, 1.0, 120.0, 30.0, [255.0; 3], [0.0; 3], 75.0);
    let bytes = square_document_with_fill_opacity(*b"lfx2", lfx2(&[bevel]), [128, 128, 128], None);
    let region = composite(bytes, Viewport::full(64, 64));
    let changed = [24, 25, 38, 39]
        .iter()
        .any(|x| sample_rgba8(&region, *x, 25)[0] != 128);
    assert!(changed, "the bevel paints a highlight or a shadow");

    let satin = satin_effect([0.0; 3], 19.0, 6.0, 5.0, false, 50.0);
    let bytes = square_document_with_fill_opacity(*b"lfx2", lfx2(&[satin]), [128, 128, 128], None);
    let region = composite(bytes, Viewport::full(64, 64));
    let changed = [25, 32, 38]
        .iter()
        .any(|x| sample_rgba8(&region, *x, 32)[0] != 128);
    assert!(changed, "the satin paints a sheen");
}

#[test]
fn a_tiled_composite_matches_the_full_composite_without_seams() {
    // The exterior stack, then an inner shadow whose distance is larger than
    // its blur. The tile split crosses the layer in both cases.
    let drop = shadow_effect(false, [0.0; 3], 120.0, 8.0, 6.0, 0.0, 75.0, true);
    let glow = glow_effect(false, false, [255.0; 3], 10.0, 20.0, 0.5, 75.0);
    let stroke = stroke_effect([255.0; 3], 3.0, false, 100.0);
    let inner = shadow_effect(true, [0.0; 3], 120.0, 20.0, 5.0, 0.0, 75.0, true);
    for items in [vec![drop, glow, stroke], vec![inner]] {
        let bytes = square_document_with_fill_opacity(*b"lfx2", lfx2(&items), [200, 60, 60], None);
        assert_the_tiles_match_the_full_image(bytes);
    }
}

/// Compare a full composite with its left and right tile halves.
fn assert_the_tiles_match_the_full_image(bytes: Vec<u8>) {
    let full = composite(bytes.clone(), Viewport::full(64, 64));
    let left = composite(bytes.clone(), Viewport::new(0, 0, 32, 64));
    let right = composite(bytes, Viewport::new(32, 0, 64, 64));
    let full_pixels = full.rgba8();
    let left_pixels = left.rgba8();
    let right_pixels = right.rgba8();
    for y in 0..64usize {
        for x in 0..64usize {
            let index = (y * 64 + x) * 4;
            let actual = if x < 32 {
                &left_pixels[(y * 32 + x) * 4..(y * 32 + x) * 4 + 4]
            } else {
                &right_pixels[(y * 32 + x - 32) * 4..(y * 32 + x - 32) * 4 + 4]
            };
            assert_eq!(
                actual,
                &full_pixels[index..index + 4],
                "pixel ({x}, {y}) differs across tiles"
            );
        }
    }
}

#[test]
fn a_group_effect_paints_over_an_isolated_group() {
    let mut spec = DocumentSpec::new(64, 64, 8, 3);
    spec.layers
        .push(LayerSpec::opaque((0, 0, 64, 64), [200, 200, 200]));
    spec.layers.push(LayerSpec::group_marker(3, *b"pass"));
    spec.layers
        .push(LayerSpec::with_alpha((24, 24, 40, 40), [200, 60, 60, 255]));
    let shadow = shadow_effect(false, [0.0; 3], 120.0, 8.0, 6.0, 0.0, 75.0, true);
    let close = LayerSpec::group_marker(1, *b"pass").with_block(*b"lfx2", lfx2(&[shadow]));
    spec.layers.push(close);
    let region = composite(build_psd(&spec), Viewport::full(64, 64));
    let backdrop = sample_rgba8(&region, 5, 5)[0];
    let shadowed = sample_rgba8(&region, 42, 45)[0];
    assert!(
        shadowed < backdrop,
        "group shadow {shadowed} is darker than {backdrop}"
    );
}

#[test]
fn the_effects_are_attached_to_the_expected_layer() {
    let drop = shadow_effect(false, [0.0; 3], 120.0, 8.0, 6.0, 0.0, 75.0, true);
    let bytes = square_document_with_fill_opacity(*b"lfx2", lfx2(&[drop]), [120, 120, 120], None);
    let document = PsdDocument::from_bytes(bytes, &ParseOptions::none()).expect("parses");
    let effects = document.file().layers[1].effects.as_ref().expect("effects");
    assert_eq!(effects.items.len(), 1);
    assert!(matches!(effects.items[0], Effect::DropShadow(_)));
    assert!(document.file().layers[0].effects.is_none());
}
