//! PSD and PSB compositor behavior on synthetic in-memory files.
//!
//! The tests build small PSD files in memory to cover the features the corpus
//! lacks: hidden layers, fill opacity, 8/16-bit parity, and the pass-through
//! versus isolated group difference.
//!
//! These tests use the generator's known pixel formulas and analytic blend
//! math.

mod common;

use common::{
    DocumentSpec, LayerSpec, brit_block, build_psd, composite_bytes, curves_block, descriptor_body,
    descriptor_item, gdfl_black_to_white, gdfl_black_to_white_midpoint, identity_levels,
    levels_block, levels_record, push_u16, sample_rgba8, scaled, soco, value_boolean,
    value_integer, versioned,
};
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
    // a = 128/255. R = 200a + 100(1-a) = 150.2, G = 99.8, B = 104.6. The
    // union of a full backdrop and a partial source rounds one step low.
    let pixel = sample_rgba8(&region, 0, 0);
    assert_eq!(&pixel[..3], &[150, 99, 104]);
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
    assert_eq!(sample_rgba8(&region, 0, 0), [78, 78, 4, 255]);
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
    assert_eq!(sample_rgba8(&region, 0, 0), [130, 174, 215, 255]);
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
    assert_eq!(sample_rgba8(&region, 0, 0), [39, 94, 239, 255]);
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
    assert_eq!(&pixel[..3], &[150, 49, 49]);
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
    assert_eq!(&pixel[..3], &[150, 49, 49]);
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
    // 100*50/255 = 19.6, truncated to 19.
    assert_eq!(sample_rgba8(&region, 0, 0), [19, 19, 19, 255]);
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
fn a_solid_color_fill_layer_paints_its_color() {
    let mut spec = DocumentSpec::new(2, 2, 8, 3);
    spec.layers
        .push(LayerSpec::opaque((0, 0, 2, 2), [100, 100, 100]));
    spec.layers
        .push(LayerSpec::content((0, 0, 2, 2)).with_block(*b"SoCo", soco(255.0, 0.0, 0.0)));
    let region = composite_bytes(build_psd(&spec));
    assert_eq!(sample_rgba8(&region, 0, 0), [255, 0, 0, 255]);
    assert_eq!(region.alpha(1, 1).expect("alpha"), 1.0);
}

#[test]
fn a_solid_fill_layer_honors_its_opacity() {
    let mut spec = DocumentSpec::new(1, 1, 8, 3);
    spec.layers.push(LayerSpec::opaque((0, 0, 1, 1), [0, 0, 0]));
    let mut fill = LayerSpec::content((0, 0, 1, 1)).with_block(*b"SoCo", soco(255.0, 255.0, 255.0));
    fill.opacity = 128;
    spec.layers.push(fill);
    let region = composite_bytes(build_psd(&spec));
    let pixel = sample_rgba8(&region, 0, 0);
    assert_eq!(&pixel[..3], &[128, 128, 128]);
    assert!(pixel[3] >= 254, "{pixel:?}");
}

#[test]
fn a_gradient_fill_layer_ramps_across_the_canvas() {
    let mut spec = DocumentSpec::new(16, 1, 8, 3);
    spec.layers
        .push(LayerSpec::content((0, 0, 1, 16)).with_block(*b"GdFl", gdfl_black_to_white(0.0)));
    let region = composite_bytes(build_psd(&spec));
    let left = sample_rgba8(&region, 0, 0);
    let middle = sample_rgba8(&region, 8, 0);
    let right = sample_rgba8(&region, 15, 0);
    assert!(
        left[0] < middle[0] && middle[0] < right[0],
        "{left:?} {middle:?} {right:?}"
    );
    assert!(left[0] < 10, "{left:?}");
    assert!(right[0] > 200, "{right:?}");
    assert_eq!(left[3], 255);
}

#[test]
fn a_gradient_midpoint_moves_the_halfway_color() {
    let mut spec = DocumentSpec::new(16, 1, 8, 3);
    spec.layers.push(
        LayerSpec::content((0, 0, 1, 16))
            .with_block(*b"GdFl", gdfl_black_to_white_midpoint(0.0, 25)),
    );
    let region = composite_bytes(build_psd(&spec));
    // The ending stop carries Mdpn 25, so the ramp is half-way at t = 0.25
    // (x = 4). A midpoint is stored on the stop that ends its segment.
    let early = sample_rgba8(&region, 4, 0);
    assert!((i32::from(early[0]) - 128).abs() <= 2, "{early:?}");
    // At t = 0.5 (x = 8) the ramp then sits two-thirds of the way up the segment.
    let middle = sample_rgba8(&region, 8, 0);
    assert!((i32::from(middle[0]) - 170).abs() <= 2, "{middle:?}");
}

#[test]
fn a_fill_layer_mask_cuts_its_coverage() {
    // A white solid fill with a mask channel that is black over the second pixel.
    let mut spec = DocumentSpec::new(2, 1, 8, 3);
    let mut fill = LayerSpec::content((0, 0, 1, 2))
        .with_block(*b"SoCo", soco(255.0, 255.0, 255.0))
        .with_mask((0, 0, 1, 2), 255);
    fill.channels = vec![(-2, vec![255, 0])];
    spec.layers.push(fill);
    let region = composite_bytes(build_psd(&spec));
    assert_eq!(sample_rgba8(&region, 0, 0), [255, 255, 255, 255]);
    assert!(region.alpha(1, 0).expect("alpha").abs() < 1e-6);
}

#[test]
fn an_invert_adjustment_changes_the_backdrop() {
    let mut spec = DocumentSpec::new(1, 1, 8, 3);
    spec.layers
        .push(LayerSpec::opaque((0, 0, 1, 1), [100, 150, 200]));
    spec.layers
        .push(LayerSpec::content((0, 0, 1, 1)).with_block(*b"nvrt", Vec::new()));
    let region = composite_bytes(build_psd(&spec));
    let color = region.color(0, 0).expect("color");
    // The complement of (100, 150, 200) over 255, before truncation.
    assert!((color[0] - 155.0 / 255.0).abs() < 1e-4, "{color:?}");
    assert!((color[1] - 105.0 / 255.0).abs() < 1e-4, "{color:?}");
    assert!((color[2] - 55.0 / 255.0).abs() < 1e-4, "{color:?}");
}

#[test]
fn an_invert_adjustment_honors_its_opacity() {
    let mut spec = DocumentSpec::new(1, 1, 8, 3);
    spec.layers
        .push(LayerSpec::opaque((0, 0, 1, 1), [100, 150, 200]));
    let mut adjustment = LayerSpec::content((0, 0, 1, 1)).with_block(*b"nvrt", Vec::new());
    adjustment.opacity = 128;
    spec.layers.push(adjustment);
    let region = composite_bytes(build_psd(&spec));
    let pixel = sample_rgba8(&region, 0, 0);
    // Halfway between 100 and 155, and so on.
    assert!((i32::from(pixel[0]) - 127).abs() <= 1, "{pixel:?}");
    assert!((i32::from(pixel[1]) - 127).abs() <= 1, "{pixel:?}");
    assert!((i32::from(pixel[2]) - 127).abs() <= 1, "{pixel:?}");
}

#[test]
fn a_levels_adjustment_maps_the_midtone() {
    let mut spec = DocumentSpec::new(1, 1, 8, 3);
    spec.layers
        .push(LayerSpec::opaque((0, 0, 1, 1), [128, 128, 128]));
    let master = levels_record(0, 255, 0, 128, 100);
    let block = levels_block(
        &master,
        &identity_levels(),
        &identity_levels(),
        &identity_levels(),
    );
    spec.layers
        .push(LayerSpec::content((0, 0, 1, 1)).with_block(*b"levl", block));
    let region = composite_bytes(build_psd(&spec));
    assert_eq!(sample_rgba8(&region, 0, 0), [64, 64, 64, 255]);
}

#[test]
fn a_curves_adjustment_maps_the_midtone() {
    let mut spec = DocumentSpec::new(1, 1, 8, 3);
    spec.layers
        .push(LayerSpec::opaque((0, 0, 1, 1), [128, 128, 128]));
    let block = curves_block(&[(0, 0), (128, 192), (255, 255)]);
    spec.layers
        .push(LayerSpec::content((0, 0, 1, 1)).with_block(*b"curv", block));
    let region = composite_bytes(build_psd(&spec));
    assert_eq!(sample_rgba8(&region, 0, 0), [192, 192, 192, 255]);
}

#[test]
fn a_brightness_contrast_adjustment_uses_the_legacy_formula() {
    let mut spec = DocumentSpec::new(1, 1, 8, 3);
    spec.layers
        .push(LayerSpec::opaque((0, 0, 1, 1), [128, 128, 128]));
    spec.layers
        .push(LayerSpec::content((0, 0, 1, 1)).with_block(*b"brit", brit_block(40, 0)));
    let region = composite_bytes(build_psd(&spec));
    assert_eq!(sample_rgba8(&region, 0, 0), [168, 168, 168, 255]);
}

#[test]
fn a_modern_brightness_contrast_descriptor_is_used() {
    // The CgEd descriptor takes precedence over the legacy `brit` pair.
    let mut spec = DocumentSpec::new(1, 1, 8, 3);
    spec.layers
        .push(LayerSpec::opaque((0, 0, 1, 1), [128, 128, 128]));
    let cged = versioned(&descriptor_body(
        b"null",
        &[
            descriptor_item(b"Brgh", &value_integer(40)),
            descriptor_item(b"Cntr", &value_integer(0)),
            descriptor_item(b"useLegacy", &value_boolean(false)),
        ],
    ));
    spec.layers.push(
        LayerSpec::content((0, 0, 1, 1))
            .with_block(*b"brit", brit_block(40, 0))
            .with_block(*b"CgEd", cged),
    );
    let region = composite_bytes(build_psd(&spec));
    let pixel = sample_rgba8(&region, 0, 0);
    assert!(pixel[0] < 165 && pixel[0] > 150, "{pixel:?}");
}

#[test]
fn a_posterize_adjustment_quantizes_the_midtone() {
    let mut spec = DocumentSpec::new(1, 1, 8, 3);
    spec.layers
        .push(LayerSpec::opaque((0, 0, 1, 1), [128, 128, 128]));
    let mut block = Vec::new();
    push_u16(&mut block, 4);
    push_u16(&mut block, 0);
    spec.layers
        .push(LayerSpec::content((0, 0, 1, 1)).with_block(*b"post", block));
    let region = composite_bytes(build_psd(&spec));
    assert_eq!(sample_rgba8(&region, 0, 0), [170, 170, 170, 255]);
}

#[test]
fn a_threshold_adjustment_splits_on_grey() {
    let mut spec = DocumentSpec::new(2, 1, 8, 3);
    let mut base = LayerSpec::content((0, 0, 1, 2));
    base.channels = vec![(0, vec![128, 64]), (1, vec![128, 64]), (2, vec![128, 64])];
    spec.layers.push(base);
    let mut block = Vec::new();
    push_u16(&mut block, 128);
    push_u16(&mut block, 0);
    spec.layers
        .push(LayerSpec::content((0, 0, 1, 2)).with_block(*b"thrs", block));
    let region = composite_bytes(build_psd(&spec));
    assert_eq!(sample_rgba8(&region, 0, 0), [255, 255, 255, 255]);
    assert_eq!(sample_rgba8(&region, 1, 0), [0, 0, 0, 255]);
}

#[test]
fn an_eight_bit_and_a_sixteen_bit_invert_agree() {
    let mut eight = DocumentSpec::new(1, 1, 8, 3);
    eight
        .layers
        .push(LayerSpec::opaque((0, 0, 1, 1), [100, 150, 200]));
    eight
        .layers
        .push(LayerSpec::content((0, 0, 1, 1)).with_block(*b"nvrt", Vec::new()));

    let mut sixteen = DocumentSpec::new(1, 1, 16, 3);
    let values = [scaled(100, 16), scaled(150, 16), scaled(200, 16)];
    sixteen.layers.push(LayerSpec::opaque((0, 0, 1, 1), values));
    sixteen
        .layers
        .push(LayerSpec::content((0, 0, 1, 1)).with_block(*b"nvrt", Vec::new()));

    let region8 = composite_bytes(build_psd(&eight));
    let region16 = composite_bytes(build_psd(&sixteen));
    assert_eq!(region8.rgba8(), region16.rgba8());
}

#[test]
fn a_sixteen_bit_curves_and_levels_match_eight_bit() {
    // Curves sample a fixed 256-entry table and Levels apply directly, so both depths
    // give the same 8-bit result even when the gamma-2.0 shadow toe is in play.
    let levels = levels_block(
        &levels_record(0, 255, 0, 255, 200),
        &identity_levels(),
        &identity_levels(),
        &identity_levels(),
    );
    let curves = curves_block(&[(0, 0), (128, 192), (255, 255)]);

    let mut eight = DocumentSpec::new(1, 1, 8, 3);
    eight
        .layers
        .push(LayerSpec::opaque((0, 0, 1, 1), [100, 150, 200]));
    eight
        .layers
        .push(LayerSpec::content((0, 0, 1, 1)).with_block(*b"levl", levels.clone()));
    eight
        .layers
        .push(LayerSpec::content((0, 0, 1, 1)).with_block(*b"curv", curves.clone()));

    let mut sixteen = DocumentSpec::new(1, 1, 16, 3);
    sixteen.layers.push(LayerSpec::opaque(
        (0, 0, 1, 1),
        [scaled(100, 16), scaled(150, 16), scaled(200, 16)],
    ));
    sixteen
        .layers
        .push(LayerSpec::content((0, 0, 1, 1)).with_block(*b"levl", levels));
    sixteen
        .layers
        .push(LayerSpec::content((0, 0, 1, 1)).with_block(*b"curv", curves));

    let region8 = composite_bytes(build_psd(&eight));
    let region16 = composite_bytes(build_psd(&sixteen));
    assert_eq!(region8.rgba8(), region16.rgba8());
}

#[test]
fn a_vector_mask_block_is_detected_but_not_applied() {
    // The parser records the block; rasterizing the path is a documented gap.
    // Both the `vmsk` and `vsms` keys mark a vector mask.
    for key in [b"vmsk", b"vsms"] {
        let mut spec = DocumentSpec::new(1, 1, 8, 3);
        spec.layers
            .push(LayerSpec::content((0, 0, 1, 1)).with_block(*key, vec![0u8; 24]));
        let document =
            PsdDocument::from_bytes(build_psd(&spec), &ParseOptions::none()).expect("parses");
        assert!(document.file().layers[0].has_vector_mask, "{key:?}");
    }
}
