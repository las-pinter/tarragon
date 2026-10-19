//! Layer-effects parsing on synthetic PSD files.

mod common;

use common::{
    DocumentSpec, LayerSpec, bevel_effect, build_psd, color_item, common as common_fields,
    descriptor_body, glow_effect, gradient_overlay_effect, legacy_lrfx, legacy_shadow_body,
    legacy_shadow_body_with_light, lfx2, lfx2_disabled, satin_effect, set_bool_item, shadow_effect,
    solid_overlay_effect, stroke_effect,
};
use tarragon_psd::{
    BevelStyle, Effect, Effects, GlowPaint, GlowSource, ParseOptions, PsdDocument, StrokePaint,
    StrokePosition,
};

/// Build a one-layer document with an effects block.
fn effects_document(key: [u8; 4], data: Vec<u8>) -> PsdDocument {
    let layer = LayerSpec::with_alpha((0, 0, 16, 16), [120, 120, 120, 255]).with_block(key, data);
    let mut spec = DocumentSpec::new(16, 16, 8, 3);
    spec.layers.push(layer);
    PsdDocument::from_bytes(build_psd(&spec), &ParseOptions::none()).expect("document parses")
}

/// Build a one-layer document with an effects block and image resources.
fn effects_document_with_resources(
    key: [u8; 4],
    data: Vec<u8>,
    resources: Vec<(u16, Vec<u8>)>,
) -> PsdDocument {
    let layer = LayerSpec::with_alpha((0, 0, 16, 16), [120, 120, 120, 255]).with_block(key, data);
    let mut spec = DocumentSpec::new(16, 16, 8, 3);
    spec.layers.push(layer);
    spec.resources = resources;
    PsdDocument::from_bytes(build_psd(&spec), &ParseOptions::none()).expect("document parses")
}

/// Build a document with one group marker, one child, and one folder record.
fn group_document(folder: LayerSpec) -> PsdDocument {
    let mut spec = DocumentSpec::new(16, 16, 8, 3);
    spec.layers.push(LayerSpec::group_marker(3, *b"pass"));
    spec.layers
        .push(LayerSpec::with_alpha((0, 0, 16, 16), [120, 120, 120, 255]));
    spec.layers.push(folder);
    PsdDocument::from_bytes(build_psd(&spec), &ParseOptions::none()).expect("document parses")
}

/// The effects of the only layer, or panic.
fn effects(document: &PsdDocument) -> &Effects {
    document
        .file()
        .layers
        .first()
        .expect("one layer")
        .effects
        .as_ref()
        .expect("effects parse")
}

#[test]
fn an_object_block_parses_every_named_effect() {
    let drop = shadow_effect(false, [0.0, 0.0, 0.0], 120.0, 8.0, 6.0, 0.0, 75.0, true);
    let inner = shadow_effect(true, [10.0, 20.0, 30.0], 30.0, 4.0, 3.0, 50.0, 60.0, true);
    let outer = glow_effect(false, false, [255.0, 220.0, 60.0], 8.0, 20.0, 50.0, 75.0);
    let glow = glow_effect(true, true, [255.0, 255.0, 255.0], 6.0, 0.0, 0.7, 70.0);
    let stroke = stroke_effect([255.0, 255.0, 255.0], 4.0, true, 100.0);
    let bevel = bevel_effect(6.0, 1.0, 120.0, 30.0, [255.0; 3], [0.0; 3], 75.0);
    let satin = satin_effect([0.0; 3], 19.0, 6.0, 5.0, false, 50.0);
    let solid = solid_overlay_effect([255.0, 0.0, 0.0], 50.0);
    let gradient = gradient_overlay_effect(0.0, 100.0);
    let data = lfx2(&[
        drop, inner, outer, glow, stroke, bevel, satin, solid, gradient,
    ]);
    let document = effects_document(*b"lfx2", data);
    let effects = effects(&document);
    assert!(effects.master);
    assert_eq!(effects.items.len(), 9);
    let mut seen = [false; 9];
    for item in &effects.items {
        match item {
            Effect::DropShadow(shadow) => {
                assert!((shadow.angle - 120.0).abs() < 1e-6);
                assert!((shadow.distance - 8.0).abs() < 1e-6);
                assert!((shadow.size - 6.0).abs() < 1e-6);
                assert!((shadow.common.opacity - 0.75).abs() < 1e-6);
                assert!(shadow.knocks_out);
                seen[0] = true;
            }
            Effect::InnerShadow(inner_shadow) => {
                assert!((inner_shadow.spread - 0.5).abs() < 1e-6);
                assert!(!inner_shadow.knocks_out);
                seen[1] = true;
            }
            Effect::OuterGlow(outer_glow) => {
                assert!(matches!(outer_glow.paint, GlowPaint::Color(_)));
                assert!((outer_glow.size - 8.0).abs() < 1e-6);
                assert_eq!(outer_glow.source, GlowSource::Edge);
                seen[2] = true;
            }
            Effect::InnerGlow(inner_glow) => {
                assert_eq!(inner_glow.source, GlowSource::Center);
                seen[3] = true;
            }
            Effect::Stroke(stroke) => {
                assert_eq!(stroke.position, StrokePosition::Inside);
                assert!(matches!(stroke.paint, StrokePaint::Color(_)));
                assert!((stroke.size - 4.0).abs() < 1e-6);
                seen[4] = true;
            }
            Effect::Bevel(bevel) => {
                assert_eq!(bevel.style, BevelStyle::Inner);
                assert!((bevel.altitude - 30.0).abs() < 1e-6);
                seen[5] = true;
            }
            Effect::Satin(satin) => {
                assert!((satin.distance - 6.0).abs() < 1e-6);
                seen[6] = true;
            }
            Effect::ColorOverlay(solid) => {
                assert!((solid.color[0] - 1.0).abs() < 1e-6);
                seen[7] = true;
            }
            Effect::GradientOverlay(gradient) => {
                assert!(gradient.gradient.stops.len() >= 2);
                seen[8] = true;
            }
        }
    }
    assert!(
        seen.iter().all(|value| *value),
        "every kind parses: {seen:?}"
    );
}

#[test]
fn the_master_switch_disables_every_item() {
    let drop = shadow_effect(false, [0.0; 3], 120.0, 8.0, 6.0, 0.0, 75.0, true);
    let document = effects_document(*b"lfx2", lfx2_disabled(&[drop]));
    let effects = effects(&document);
    assert!(!effects.master);
    assert!(!effects.is_active());
    assert_eq!(effects.margin(), 0.0);
}

#[test]
fn an_older_lmfx_block_parses_the_same_way() {
    let solid = solid_overlay_effect([0.0, 0.0, 255.0], 100.0);
    let document = effects_document(*b"lmfx", lfx2(&[solid]));
    let effects = effects(&document);
    assert_eq!(effects.items.len(), 1);
    assert!(matches!(effects.items[0], Effect::ColorOverlay(_)));
}

#[test]
fn a_legacy_block_parses_a_drop_shadow() {
    let data = legacy_lrfx(&[(*b"dsdw", legacy_shadow_body(120.0, 8.0, 6.0, 191, true))]);
    let document = effects_document(*b"lrFX", data);
    let effects = effects(&document);
    assert_eq!(effects.items.len(), 1);
    let Effect::DropShadow(shadow) = &effects.items[0] else {
        panic!("legacy drop shadow");
    };
    assert!((shadow.angle - 120.0).abs() < 1e-3);
    assert!((shadow.distance - 8.0).abs() < 1e-3);
    assert!((shadow.size - 6.0).abs() < 1e-3);
    assert!((shadow.common.opacity - 191.0 / 255.0).abs() < 1e-6);
}

#[test]
fn a_legacy_shadow_reads_its_global_light_flag() {
    let cases = [(true, true), (false, false)];
    for (use_global, expected) in cases {
        let data = legacy_lrfx(&[(
            *b"dsdw",
            legacy_shadow_body_with_light(120.0, 8.0, 6.0, 191, true, use_global),
        )]);
        let document = effects_document(*b"lrFX", data);
        let Effect::DropShadow(shadow) = &effects(&document).items[0] else {
            panic!("legacy drop shadow");
        };
        assert_eq!(shadow.use_global_light, expected);
    }
}

#[test]
fn a_bevel_reads_its_global_light_flag() {
    let mut bevel = bevel_effect(6.0, 1.0, 120.0, 30.0, [255.0; 3], [0.0; 3], 75.0);
    set_bool_item(&mut bevel.1, b"uglg", true);
    let document = effects_document(*b"lfx2", lfx2(&[bevel]));
    let Effect::Bevel(bevel) = &effects(&document).items[0] else {
        panic!("bevel");
    };
    assert!(bevel.use_global_light);
}

#[test]
fn a_legacy_common_state_hides_every_item() {
    let mut common_state = Vec::new();
    common_state.extend_from_slice(&0u32.to_be_bytes());
    common_state.push(0);
    common_state.extend_from_slice(&[0, 0]);
    let data = legacy_lrfx(&[
        (*b"cmnS", common_state),
        (*b"dsdw", legacy_shadow_body(120.0, 8.0, 6.0, 191, true)),
    ]);
    let document = effects_document(*b"lrFX", data);
    let effects = effects(&document);
    assert!(!effects.master);
}

#[test]
fn a_disabled_item_keeps_its_fields_but_does_not_activate() {
    let mut items = common_fields(false, b"Mltp", 75.0);
    items.push(common_color(0.0, 0.0, 0.0));
    let key = *b"DrSh";
    let body = descriptor_body(&key, &items);
    let document = effects_document(*b"lfx2", lfx2(&[(key, body)]));
    let effects = effects(&document);
    assert_eq!(effects.items.len(), 1);
    assert!(!effects.items[0].is_enabled());
    assert!(!effects.is_active());
}

/// A color item for a shadow descriptor.
fn common_color(red: f64, green: f64, blue: f64) -> Vec<u8> {
    color_item(b"Clr ", red, green, blue)
}

#[test]
fn garbage_effect_bytes_parse_as_absent_without_panic() {
    let document = effects_document(*b"lfx2", vec![0xde, 0xad, 0xbe, 0xef, 0x00]);
    assert!(
        document.file().layers[0].effects.is_none(),
        "a bad block is not an effect"
    );
}

#[test]
fn the_document_global_light_comes_from_the_image_resources() {
    let mut drop = shadow_effect(false, [0.0; 3], 120.0, 8.0, 6.0, 0.0, 75.0, true);
    set_bool_item(&mut drop.1, b"uglg", true);
    let document = effects_document_with_resources(
        *b"lfx2",
        lfx2(&[drop]),
        vec![
            (1037, 0i32.to_be_bytes().to_vec()),
            (1038, 60i32.to_be_bytes().to_vec()),
        ],
    );
    let file = document.file();
    assert!((file.global_light.angle - 0.0).abs() < 1e-6);
    assert!((file.global_light.altitude - 60.0).abs() < 1e-6);
    let Effect::DropShadow(shadow) = &effects(&document).items[0] else {
        panic!("drop shadow");
    };
    // The effect keeps its own angle and flags that it follows the document.
    assert!((shadow.angle - 120.0).abs() < 1e-6);
    assert!(shadow.use_global_light);
}

#[test]
fn a_false_present_field_drops_the_effect() {
    let (key, mut body) = shadow_effect(false, [0.0; 3], 120.0, 8.0, 6.0, 0.0, 75.0, true);
    set_bool_item(&mut body, b"present", false);
    let document = effects_document(*b"lfx2", lfx2(&[(key, body)]));
    let effects = effects(&document);
    assert!(effects.items.is_empty(), "the effect is dropped");
    assert!(!effects.is_active());
}

#[test]
fn an_older_lmfx_block_wins_over_lfx2_in_one_record() {
    let solid = solid_overlay_effect([0.0, 0.0, 255.0], 100.0);
    let drop = shadow_effect(false, [0.0; 3], 120.0, 8.0, 6.0, 0.0, 75.0, true);
    let layer = LayerSpec::with_alpha((0, 0, 16, 16), [120, 120, 120, 255])
        .with_block(*b"lfx2", lfx2(&[drop]))
        .with_block(*b"lmfx", lfx2(&[solid]));
    let mut spec = DocumentSpec::new(16, 16, 8, 3);
    spec.layers.push(layer);
    let document =
        PsdDocument::from_bytes(build_psd(&spec), &ParseOptions::none()).expect("document parses");
    let effects = effects(&document);
    assert_eq!(effects.items.len(), 1);
    assert!(
        matches!(effects.items[0], Effect::ColorOverlay(_)),
        "the lmfx block wins over lfx2"
    );
}

#[test]
fn a_group_folder_reads_its_lfxs_block() {
    let drop = shadow_effect(false, [0.0; 3], 120.0, 8.0, 6.0, 0.0, 75.0, true);
    let folder = LayerSpec::group_marker(1, *b"pass").with_block(*b"lfxs", lfx2(&[drop]));
    let document = group_document(folder);
    let effects = document.file().layers[2]
        .effects
        .as_ref()
        .expect("group effects parse");
    assert_eq!(effects.items.len(), 1);
    assert!(matches!(effects.items[0], Effect::DropShadow(_)));
}

#[test]
fn a_legacy_block_on_a_group_folder_is_ignored() {
    let data = legacy_lrfx(&[(*b"dsdw", legacy_shadow_body(120.0, 8.0, 6.0, 191, true))]);
    let folder = LayerSpec::group_marker(1, *b"pass").with_block(*b"lrFX", data);
    let document = group_document(folder);
    assert!(
        document.file().layers[2].effects.is_none(),
        "Photoshop ignores the legacy block on a group folder"
    );
}

#[test]
fn every_effect_block_prefix_parses_or_is_absent_without_panic() {
    let drop = shadow_effect(false, [0.0; 3], 120.0, 8.0, 6.0, 0.0, 75.0, true);
    let full = lfx2(&[drop]);
    for cut in (0..full.len()).step_by(11) {
        let document = effects_document(*b"lfx2", full[..cut].to_vec());
        // A cut block is absent or short; the point is that it never panics.
        let _ = document.file().layers[0].effects.as_ref();
    }
}
