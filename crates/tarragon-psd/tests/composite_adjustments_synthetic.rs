//! Fill and adjustment layers on synthetic PSD files.
//!
//! These cover solid and gradient fills, fill masks, and the adjustment set
//! (invert, levels, curves, brightness/contrast, posterize, threshold), at
//! 8-bit and 16-bit depth.

mod common;

use common::{
    DocumentSpec, LayerSpec, brit_block, build_psd, composite_bytes, curves_block, descriptor_body,
    descriptor_item, gdfl_black_to_white, gdfl_black_to_white_midpoint, identity_levels,
    levels_block, levels_record, push_u16, sample_rgba8, scaled, soco, value_boolean,
    value_integer, versioned,
};

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
