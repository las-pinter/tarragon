//! Hand-built layer-effects fixture.
//!
//! The committed `layer_effects.psd` was built with the PSD writer rules in
//! this test builder, because the pinned generator interpreter is not
//! available in every environment. The generator in
//! `tests/fixtures/psd/generate.py` writes the same document shape, not
//! byte-identical bytes. These descriptor and record fields differ:
//!
//! - this builder writes a linear transfer contour (`TrnS`) on the shadows,
//!   the glows, and the bevel, and the satin curve (`MpgS`); the generator
//!   omits them.
//! - the Rust builder can write the inner-glow source enum (`glwS`); the
//!   generator never writes it.
//! - this builder writes the bevel depth (`srgR`) as 1 percent; the generator
//!   writes 100 percent.
//! - this builder writes a Pascal layer name only; the generator also writes
//!   a Unicode layer name block.
//! - this builder writes empty blending ranges; the generator writes the
//!   default range block.
//! - this builder writes raw channel rows; the generator writes RLE rows.
//!
//! The check test keeps the committed bytes and the builder in step. It does
//! not compare the file with a Photoshop render. The ignored test rewrites the
//! file after a builder change:
//!
//! ```text
//! cargo test -p tarragon-psd --test layer_effects_fixture -- --ignored
//! ```

mod common;

use common::{
    DocumentSpec, LayerSpec, bevel_effect, build_psd, glow_effect, gradient_overlay_effect, lfx2,
    satin_effect, shadow_effect, solid_overlay_effect, stroke_effect,
};

/// One top-level effect item.
type EffectItem = ([u8; 4], Vec<u8>);
/// One fixture cell: name, column, row, color, and effects.
type Cell = (&'static str, i32, i32, [u16; 3], Vec<EffectItem>);

/// Build the layer-effects fixture bytes.
fn layer_effects_bytes() -> Vec<u8> {
    let size = 192u32;
    let mut spec = DocumentSpec::new(size, size, 8, 3);
    let mut background = LayerSpec::opaque((0, 0, 192, 192), [230, 230, 230]);
    background.name = Some("Backdrop".to_string());
    spec.layers.push(background);
    let cells: Vec<Cell> = vec![
        (
            "Drop Shadow",
            0,
            0,
            [200, 60, 60],
            vec![shadow_effect(
                false, [0.0; 3], 120.0, 8.0, 6.0, 0.0, 75.0, true,
            )],
        ),
        (
            "Inner Shadow",
            1,
            0,
            [60, 120, 200],
            vec![shadow_effect(
                true, [0.0; 3], 120.0, 6.0, 5.0, 0.0, 75.0, true,
            )],
        ),
        (
            "Outer Glow",
            2,
            0,
            [60, 180, 90],
            vec![glow_effect(
                false,
                false,
                [255.0, 220.0, 60.0],
                8.0,
                20.0,
                50.0,
                75.0,
            )],
        ),
        (
            "Inner Glow",
            0,
            1,
            [180, 140, 60],
            vec![glow_effect(true, false, [255.0; 3], 6.0, 0.0, 50.0, 70.0)],
        ),
        (
            "Stroke",
            1,
            1,
            [120, 120, 220],
            vec![stroke_effect([255.0; 3], 4.0, true, 100.0)],
        ),
        (
            "Bevel",
            2,
            1,
            [140, 140, 140],
            vec![bevel_effect(
                6.0, 1.0, 120.0, 30.0, [255.0; 3], [0.0; 3], 75.0,
            )],
        ),
        (
            "Satin",
            0,
            2,
            [120, 120, 120],
            vec![satin_effect([0.0; 3], 19.0, 6.0, 5.0, false, 50.0)],
        ),
        (
            "Color Overlay",
            1,
            2,
            [100, 100, 100],
            vec![solid_overlay_effect([255.0, 0.0, 0.0], 50.0)],
        ),
        (
            "Gradient Overlay",
            2,
            2,
            [100, 100, 100],
            vec![gradient_overlay_effect(0.0, 100.0)],
        ),
    ];
    for (name, column, row, color, effects) in cells {
        let left = 16 + column * 64;
        let top = 16 + row * 64;
        let mut layer = LayerSpec::with_alpha(
            (top, left, top + 32, left + 32),
            [color[0], color[1], color[2], 255],
        )
        .with_block(*b"lfx2", lfx2(&effects));
        layer.name = Some(name.to_string());
        spec.layers.push(layer);
    }
    build_psd(&spec)
}

#[test]
fn the_committed_layer_effects_fixture_matches_the_builder() {
    let committed = std::fs::read(common::fixture("layer_effects.psd")).expect("fixture reads");
    assert_eq!(
        committed,
        layer_effects_bytes(),
        "regenerate the fixture after a builder change"
    );
}

#[test]
#[ignore = "rewrites the committed layer-effects fixture"]
fn regenerate_layer_effects_fixture() {
    std::fs::write(common::fixture("layer_effects.psd"), layer_effects_bytes())
        .expect("fixture writes");
}
