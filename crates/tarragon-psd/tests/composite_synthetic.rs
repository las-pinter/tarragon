//! PSD and PSB compositor behavior on synthetic in-memory files.
//!
//! The tests build small PSD files in memory to cover the features the corpus
//! lacks: hidden layers, fill opacity, 8/16-bit parity, and the pass-through
//! versus isolated group difference. The fill and adjustment tests are in
//! `composite_adjustments_synthetic.rs`.
//!
//! These tests use the generator's known pixel formulas and analytic blend
//! math.

mod common;

use common::{DocumentSpec, LayerSpec, build_psd, composite_bytes, sample_rgba8, scaled};
use tarragon_psd::blend::BlendMode;
use tarragon_psd::{ParseOptions, PsdDocument};

// --------------------------------------------------------------------------
// Synthetic tests
// --------------------------------------------------------------------------

#[test]
fn a_normal_layer_composites_over_an_opaque_background() {
    let mut spec = DocumentSpec::new(2, 2, 8, 3);
    spec.layers
        .push(LayerSpec::opaque((0, 0, 2, 2), [100, 150, 200]));
    let mut top = LayerSpec::with_alpha((0, 0, 2, 2), [200, 50, 10, 128]);
    top.blend = *b"norm";
    spec.layers.push(top);
    let region = composite_bytes(build_psd(&spec));
    // a = 128/255. R = 200a + 100(1-a) = 150.2, G = 99.8, B = 104.6. Each
    // rounds to the nearest level.
    let pixel = sample_rgba8(&region, 0, 0);
    assert_eq!(&pixel[..3], &[150, 100, 105]);
    assert!(pixel[3] >= 254, "opaque alpha, got {}", pixel[3]);
}

#[test]
fn a_multiply_layer_uses_the_multiply_formula() {
    let mut spec = DocumentSpec::new(2, 2, 8, 3);
    spec.layers
        .push(LayerSpec::opaque((0, 0, 2, 2), [200, 100, 50]));
    let mut top = LayerSpec::opaque((0, 0, 2, 2), [100, 200, 25]);
    top.blend = *b"mul ";
    spec.layers.push(top);
    let region = composite_bytes(build_psd(&spec));
    // 200*100/255 = 78.4, 100*200/255 = 78.4, 50*25/255 = 4.9.
    assert_eq!(sample_rgba8(&region, 0, 0), [78, 78, 5, 255]);
}

#[test]
fn a_screen_layer_uses_the_screen_formula() {
    let mut spec = DocumentSpec::new(2, 2, 8, 3);
    spec.layers
        .push(LayerSpec::opaque((0, 0, 2, 2), [100, 150, 200]));
    let mut top = LayerSpec::opaque((0, 0, 2, 2), [50, 60, 70]);
    top.blend = *b"scrn";
    spec.layers.push(top);
    let region = composite_bytes(build_psd(&spec));
    assert_eq!(sample_rgba8(&region, 0, 0), [130, 175, 215, 255]);
}

#[test]
fn an_overlay_layer_uses_the_overlay_formula() {
    let mut spec = DocumentSpec::new(2, 2, 8, 3);
    spec.layers
        .push(LayerSpec::opaque((0, 0, 2, 2), [100, 150, 200]));
    let mut top = LayerSpec::opaque((0, 0, 2, 2), [50, 60, 220]);
    top.blend = *b"over";
    spec.layers.push(top);
    let region = composite_bytes(build_psd(&spec));
    assert_eq!(sample_rgba8(&region, 0, 0), [39, 94, 240, 255]);
}

#[test]
fn a_linear_dodge_layer_saturates_the_channels() {
    let mut spec = DocumentSpec::new(2, 2, 8, 3);
    spec.layers
        .push(LayerSpec::opaque((0, 0, 2, 2), [100, 150, 200]));
    let mut top = LayerSpec::opaque((0, 0, 2, 2), [200, 100, 55]);
    top.blend = *b"lddg";
    spec.layers.push(top);
    let region = composite_bytes(build_psd(&spec));
    assert_eq!(sample_rgba8(&region, 0, 0), [255, 250, 255, 255]);
}

#[test]
fn a_layer_opacity_scales_the_source_alpha() {
    let mut spec = DocumentSpec::new(2, 2, 8, 3);
    spec.layers
        .push(LayerSpec::opaque((0, 0, 2, 2), [100, 100, 100]));
    let mut top = LayerSpec::opaque((0, 0, 2, 2), [200, 0, 0]);
    top.opacity = 128;
    spec.layers.push(top);
    let region = composite_bytes(build_psd(&spec));
    let pixel = sample_rgba8(&region, 0, 0);
    assert_eq!(&pixel[..3], &[150, 50, 50]);
    assert!(pixel[3] >= 254, "opaque alpha, got {}", pixel[3]);
}

#[test]
fn a_fill_opacity_scales_the_source_alpha() {
    let mut spec = DocumentSpec::new(2, 2, 8, 3);
    spec.layers
        .push(LayerSpec::opaque((0, 0, 2, 2), [100, 100, 100]));
    let mut top = LayerSpec::opaque((0, 0, 2, 2), [200, 0, 0]);
    top.fill_opacity = Some(128);
    spec.layers.push(top);
    let region = composite_bytes(build_psd(&spec));
    let pixel = sample_rgba8(&region, 0, 0);
    assert_eq!(&pixel[..3], &[150, 50, 50]);
    assert!(pixel[3] >= 254, "opaque alpha, got {}", pixel[3]);
}

#[test]
fn a_hidden_layer_changes_nothing() {
    let mut visible = DocumentSpec::new(2, 2, 8, 3);
    visible
        .layers
        .push(LayerSpec::opaque((0, 0, 2, 2), [100, 150, 200]));
    let baseline = composite_bytes(build_psd(&visible));

    let mut hidden = DocumentSpec::new(2, 2, 8, 3);
    hidden
        .layers
        .push(LayerSpec::opaque((0, 0, 2, 2), [100, 150, 200]));
    let mut top = LayerSpec::opaque((0, 0, 2, 2), [200, 50, 10]);
    top.flags = 0x02;
    hidden.layers.push(top);
    let with_hidden = composite_bytes(build_psd(&hidden));

    assert_eq!(with_hidden.rgba8(), baseline.rgba8());
    assert_eq!(sample_rgba8(&with_hidden, 0, 0), [100, 150, 200, 255]);
}

#[test]
fn a_pass_through_group_blends_onto_the_backdrop() {
    // Background (100,100,100) and a pass-through group with one multiply
    // child (50,50,50). The child multiplies with the backdrop.
    let mut spec = DocumentSpec::new(2, 2, 8, 3);
    spec.layers
        .push(LayerSpec::opaque((0, 0, 2, 2), [100, 100, 100]));
    spec.layers.push(LayerSpec::group_marker(3, *b"pass"));
    let mut child = LayerSpec::opaque((0, 0, 2, 2), [50, 50, 50]);
    child.blend = *b"mul ";
    spec.layers.push(child);
    spec.layers.push(LayerSpec::group_marker(1, *b"pass"));
    let region = composite_bytes(build_psd(&spec));
    // 100*50/255 = 19.6, which rounds to 20.
    assert_eq!(sample_rgba8(&region, 0, 0), [20, 20, 20, 255]);
}

#[test]
fn an_isolated_group_blends_onto_a_transparent_buffer() {
    // The same layers, but the group is normal (isolated). The multiply child
    // sees a transparent buffer, so it keeps its own color.
    let mut spec = DocumentSpec::new(2, 2, 8, 3);
    spec.layers
        .push(LayerSpec::opaque((0, 0, 2, 2), [100, 100, 100]));
    spec.layers.push(LayerSpec::group_marker(3, *b"norm"));
    let mut child = LayerSpec::opaque((0, 0, 2, 2), [50, 50, 50]);
    child.blend = *b"mul ";
    spec.layers.push(child);
    spec.layers.push(LayerSpec::group_marker(1, *b"norm"));
    let region = composite_bytes(build_psd(&spec));
    assert_eq!(sample_rgba8(&region, 0, 0), [50, 50, 50, 255]);
}

#[test]
fn a_clipping_layer_is_restricted_to_the_base_alpha() {
    // Base: opaque color with alpha 128 on the left pixel and 0 on the right.
    // The clip layer covers both pixels, but the right pixel must stay clear.
    let mut spec = DocumentSpec::new(2, 1, 8, 3);
    let mut base = LayerSpec::opaque((0, 0, 1, 2), [200, 200, 200]);
    base.channels.push((-1, vec![128, 0]));
    spec.layers.push(base);
    let mut clip = LayerSpec::opaque((0, 0, 1, 2), [0, 0, 255]);
    clip.clipping = 1;
    spec.layers.push(clip);
    let region = composite_bytes(build_psd(&spec));
    let left = region.alpha(0, 0).expect("left alpha");
    assert!(left > 0.4, "the base alpha shows through: {left}");
    let right = region.alpha(1, 0).expect("right alpha");
    assert!(right.abs() < 1e-6, "the clip is cut by the base: {right}");
}

#[test]
fn a_layer_below_the_viewport_top_writes_its_first_row_at_its_own_offset() {
    // The layer starts at canvas row 5. A viewport from row 0 must write the
    // layer's first row at output row 5, not at row 0.
    let mut spec = DocumentSpec::new(8, 8, 8, 3);
    spec.layers
        .push(LayerSpec::opaque((0, 0, 8, 8), [200, 200, 200]));
    spec.layers
        .push(LayerSpec::opaque((5, 0, 8, 8), [10, 20, 30]));
    let region = composite_bytes(build_psd(&spec));
    assert_eq!(sample_rgba8(&region, 0, 4), [200, 200, 200, 255]);
    assert_eq!(
        sample_rgba8(&region, 0, 5),
        [10, 20, 30, 255],
        "the layer starts at output row 5"
    );
}

#[test]
fn an_eight_bit_and_a_sixteen_bit_image_agree() {
    let mut eight = DocumentSpec::new(2, 1, 8, 3);
    eight
        .layers
        .push(LayerSpec::opaque((0, 0, 1, 2), [100, 150, 200]));
    let mut eight_top = LayerSpec::with_alpha((0, 0, 1, 2), [200, 60, 30, 128]);
    eight_top.blend = *b"mul ";
    eight.layers.push(eight_top);

    let mut sixteen = DocumentSpec::new(2, 1, 16, 3);
    let background = [scaled(100, 16), scaled(150, 16), scaled(200, 16)];
    sixteen
        .layers
        .push(LayerSpec::opaque((0, 0, 1, 2), background));
    let top = [
        scaled(200, 16),
        scaled(60, 16),
        scaled(30, 16),
        scaled(128, 16),
    ];
    let mut sixteen_top = LayerSpec::with_alpha((0, 0, 1, 2), top);
    sixteen_top.blend = *b"mul ";
    sixteen.layers.push(sixteen_top);

    let region8 = composite_bytes(build_psd(&eight));
    let region16 = composite_bytes(build_psd(&sixteen));
    assert_eq!(region8.rgba8(), region16.rgba8());
}

#[test]
fn a_flattened_document_uses_the_merged_background() {
    let mut spec = DocumentSpec::new(2, 2, 8, 3);
    // A merged gradient: R = x, G = y, B = 10. There are no layers.
    spec.merged = vec![vec![0, 1, 0, 1], vec![0, 0, 1, 1], vec![10, 10, 10, 10]];
    let region = composite_bytes(build_psd(&spec));
    assert_eq!(sample_rgba8(&region, 0, 0), [0, 0, 10, 255]);
    assert_eq!(sample_rgba8(&region, 1, 1), [1, 1, 10, 255]);
}

#[test]
fn a_blend_mode_unit_covers_every_named_mode() {
    // The fixture corpus covers a few modes. This checks that the public blend
    // entry returns a value for every mode without panicking.
    let modes = [
        BlendMode::PassThrough,
        BlendMode::Normal,
        BlendMode::Dissolve,
        BlendMode::Darken,
        BlendMode::Multiply,
        BlendMode::ColorBurn,
        BlendMode::LinearBurn,
        BlendMode::DarkerColor,
        BlendMode::Lighten,
        BlendMode::Screen,
        BlendMode::ColorDodge,
        BlendMode::LinearDodge,
        BlendMode::LighterColor,
        BlendMode::Overlay,
        BlendMode::SoftLight,
        BlendMode::HardLight,
        BlendMode::VividLight,
        BlendMode::LinearLight,
        BlendMode::PinLight,
        BlendMode::HardMix,
        BlendMode::Difference,
        BlendMode::Exclusion,
        BlendMode::Subtract,
        BlendMode::Divide,
        BlendMode::Hue,
        BlendMode::Saturation,
        BlendMode::Color,
        BlendMode::Luminosity,
    ];
    for mode in modes {
        let out = tarragon_psd::blend::blend_rgb(mode, [0.4, 0.5, 0.6], [0.6, 0.3, 0.2]);
        for channel in out {
            assert!((0.0..=1.0).contains(&channel), "{mode:?}");
        }
    }
}

#[test]
fn vivid_light_lets_a_source_extreme_win_over_the_backdrop() {
    // A black source over a white backdrop is black, not white.
    let mut spec = DocumentSpec::new(1, 1, 8, 3);
    spec.layers
        .push(LayerSpec::opaque((0, 0, 1, 1), [255, 255, 255]));
    let mut source = LayerSpec::opaque((0, 0, 1, 1), [0, 0, 0]);
    source.blend = *b"vLit";
    spec.layers.push(source);
    let region = composite_bytes(build_psd(&spec));
    assert_eq!(sample_rgba8(&region, 0, 0), [0, 0, 0, 255]);
}

#[test]
fn hard_mix_keeps_its_photoshop_corner_cases() {
    // A white backdrop under a black source stays white for Hard Mix.
    let mut light = DocumentSpec::new(1, 1, 8, 3);
    light
        .layers
        .push(LayerSpec::opaque((0, 0, 1, 1), [255, 255, 255]));
    let mut dark_source = LayerSpec::opaque((0, 0, 1, 1), [0, 0, 0]);
    dark_source.blend = *b"hMix";
    light.layers.push(dark_source);
    let region = composite_bytes(build_psd(&light));
    assert_eq!(sample_rgba8(&region, 0, 0), [255, 255, 255, 255]);

    // A black backdrop under a white source stays black.
    let mut dark = DocumentSpec::new(1, 1, 8, 3);
    dark.layers.push(LayerSpec::opaque((0, 0, 1, 1), [0, 0, 0]));
    let mut light_source = LayerSpec::opaque((0, 0, 1, 1), [255, 255, 255]);
    light_source.blend = *b"hMix";
    dark.layers.push(light_source);
    let region = composite_bytes(build_psd(&dark));
    assert_eq!(sample_rgba8(&region, 0, 0), [0, 0, 0, 255]);
}

#[test]
fn a_vector_mask_block_is_detected_even_when_its_path_is_bad() {
    // Both the `vmsk` and `vsms` keys mark a vector mask. This body has the
    // wrong version, so it gives no path. The rasterizer tests are in
    // `composite_vector_mask.rs`.
    for key in [b"vmsk", b"vsms"] {
        let mut spec = DocumentSpec::new(1, 1, 8, 3);
        spec.layers
            .push(LayerSpec::content((0, 0, 1, 1)).with_block(*key, vec![0u8; 24]));
        let document =
            PsdDocument::from_bytes(build_psd(&spec), &ParseOptions::none()).expect("parses");
        assert!(document.file().layers[0].has_vector_mask, "{key:?}");
    }
}

// --------------------------------------------------------------------------
// Fill opacity in the special-eight modes
// --------------------------------------------------------------------------

/// The special-eight keys with the result at fill 128 of a (100,180,220) layer
/// over a (200,150,100) backdrop, measured on Photoshop to within one level.
const SPECIAL_EIGHT: [([u8; 4], [u8; 3]); 8] = [
    (*b"idiv", [176, 132, 88]),
    (*b"lbrn", [122, 112, 82]),
    (*b"div ", [249, 232, 176]),
    (*b"lddg", [250, 240, 210]),
    (*b"vLit", [193, 188, 157]),
    (*b"lLit", [171, 202, 192]),
    (*b"hMix", [245, 225, 165]),
    (*b"diff", [150, 60, 10]),
];

/// A backdrop with one top layer at the given blend, opacity and fill.
fn fill_document(blend: [u8; 4], opacity: u8, fill: u8) -> DocumentSpec {
    let mut spec = DocumentSpec::new(2, 2, 8, 3);
    spec.layers
        .push(LayerSpec::opaque((0, 0, 2, 2), [200, 150, 100]));
    let mut top = LayerSpec::opaque((0, 0, 2, 2), [100, 180, 220]);
    top.blend = blend;
    top.opacity = opacity;
    top.fill_opacity = Some(fill);
    spec.layers.push(top);
    spec
}

/// Assert that a pixel is within a tolerance of the expected colour.
fn assert_near(pixel: [u8; 4], want: [f32; 3], tolerance: f32, label: &str) {
    for channel in 0..3 {
        let got = f32::from(pixel[channel]);
        assert!(
            (got - want[channel]).abs() <= tolerance,
            "{label} channel {channel}: {got} vs {}",
            want[channel]
        );
    }
}

#[test]
fn the_special_eight_use_fill_as_photoshop_does_on_an_opaque_backdrop() {
    for (blend, want) in SPECIAL_EIGHT {
        let region = composite_bytes(build_psd(&fill_document(blend, 255, 128)));
        let pixel = sample_rgba8(&region, 0, 0);
        let want = want.map(f32::from);
        assert_near(pixel, want, 2.0, &format!("{blend:?}"));
        assert!(pixel[3] >= 254, "opaque alpha, got {}", pixel[3]);
    }
}

#[test]
fn the_special_eight_at_fill_zero_leave_the_backdrop() {
    for (blend, _) in SPECIAL_EIGHT {
        let region = composite_bytes(build_psd(&fill_document(blend, 255, 0)));
        assert_eq!(
            sample_rgba8(&region, 0, 0),
            [200, 150, 100, 255],
            "{blend:?}"
        );
    }
}

#[test]
fn the_special_eight_at_full_fill_are_the_plain_blend() {
    for (blend, _) in SPECIAL_EIGHT {
        let plain = composite_bytes(build_psd(&fill_document(blend, 255, 255)));
        let mut spec = fill_document(blend, 255, 255);
        spec.layers[1].fill_opacity = None;
        let absent = composite_bytes(build_psd(&spec));
        assert_eq!(plain.rgba8(), absent.rgba8(), "{blend:?}");
    }
}

#[test]
fn the_special_eight_keep_fill_as_coverage_over_a_transparent_backdrop() {
    for (blend, _) in SPECIAL_EIGHT {
        let mut spec = DocumentSpec::new(2, 2, 8, 3);
        let mut top = LayerSpec::opaque((0, 0, 2, 2), [100, 180, 220]);
        top.blend = blend;
        top.fill_opacity = Some(128);
        spec.layers.push(top);
        let region = composite_bytes(build_psd(&spec));
        let pixel = sample_rgba8(&region, 0, 0);
        assert_near(pixel, [100.0, 180.0, 220.0], 1.0, &format!("{blend:?}"));
        assert!(
            (i32::from(pixel[3]) - 128).abs() <= 1,
            "{blend:?} alpha {}",
            pixel[3]
        );
    }
}

#[test]
fn opacity_mixes_the_special_eight_fill_blend_in() {
    // Linear dodge at fill f adds the source scaled by f. Opacity o then mixes
    // that blend colour with the backdrop at weight o.
    let (f, o) = (128.0f32 / 255.0, 128.0f32 / 255.0);
    let backdrop = [200.0f32, 150.0, 100.0];
    let source = [100.0f32, 180.0, 220.0];
    let want: [f32; 3] = std::array::from_fn(|i| {
        let blend = (backdrop[i] + source[i] * f).min(255.0);
        backdrop[i] + (blend - backdrop[i]) * o
    });
    let region = composite_bytes(build_psd(&fill_document(*b"lddg", 128, 128)));
    assert_near(sample_rgba8(&region, 0, 0), want, 1.5, "linear dodge");
}

#[test]
fn a_group_in_a_special_eight_mode_uses_fill_as_photoshop_does() {
    let mut spec = DocumentSpec::new(2, 2, 8, 3);
    spec.layers
        .push(LayerSpec::opaque((0, 0, 2, 2), [200, 150, 100]));
    spec.layers.push(LayerSpec::group_marker(3, *b"norm"));
    spec.layers
        .push(LayerSpec::opaque((0, 0, 2, 2), [100, 180, 220]));
    let mut close = LayerSpec::group_marker(1, *b"lddg");
    close.fill_opacity = Some(128);
    spec.layers.push(close);
    let region = composite_bytes(build_psd(&spec));
    assert_near(
        sample_rgba8(&region, 0, 0),
        [250.0, 240.0, 210.0],
        2.0,
        "group",
    );
}
