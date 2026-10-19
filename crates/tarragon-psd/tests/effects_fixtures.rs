//! Layer-effects fixture tests.
//!
//! The committed `layer_effects.psd` carries nine effect layers over a light
//! backdrop. Each test reads one cell and checks the effect rule the cell
//! demonstrates.

mod common;

use common::{composite_fixture, sample_rgba8};
use tarragon_psd::{Effect, ParseOptions, PsdDocument, StrokePosition, Viewport};

/// Composite the fixture over the full canvas.
fn region() -> tarragon_psd::CompositeRegion {
    composite_fixture("layer_effects.psd")
}

#[test]
fn the_fixture_names_and_parses_every_effect_kind() {
    let document = PsdDocument::open(&common::fixture("layer_effects.psd")).expect("fixture opens");
    let file = document.file();
    assert_eq!(file.layers.len(), 10);
    assert_eq!(file.layers[0].name, "Backdrop");
    let expected = [
        "Drop Shadow",
        "Inner Shadow",
        "Outer Glow",
        "Inner Glow",
        "Stroke",
        "Bevel",
        "Satin",
        "Color Overlay",
        "Gradient Overlay",
    ];
    for (index, name) in expected.iter().enumerate() {
        assert_eq!(file.layers[index + 1].name, *name);
        let effects = file.layers[index + 1]
            .effects
            .as_ref()
            .unwrap_or_else(|| panic!("{name} has effects"));
        assert!(effects.is_active(), "{name} is active");
        assert_eq!(effects.items.len(), 1, "{name} has one item");
    }
    assert!(matches!(
        file.layers[1].effects.as_ref().unwrap().items[0],
        Effect::DropShadow(_)
    ));
    assert!(matches!(
        file.layers[2].effects.as_ref().unwrap().items[0],
        Effect::InnerShadow(_)
    ));
    assert!(matches!(
        file.layers[3].effects.as_ref().unwrap().items[0],
        Effect::OuterGlow(_)
    ));
    assert!(matches!(
        file.layers[4].effects.as_ref().unwrap().items[0],
        Effect::InnerGlow(_)
    ));
    assert!(matches!(
        file.layers[5].effects.as_ref().unwrap().items[0],
        Effect::Stroke(_)
    ));
    assert!(matches!(
        file.layers[6].effects.as_ref().unwrap().items[0],
        Effect::Bevel(_)
    ));
    assert!(matches!(
        file.layers[7].effects.as_ref().unwrap().items[0],
        Effect::Satin(_)
    ));
    assert!(matches!(
        file.layers[8].effects.as_ref().unwrap().items[0],
        Effect::ColorOverlay(_)
    ));
    assert!(matches!(
        file.layers[9].effects.as_ref().unwrap().items[0],
        Effect::GradientOverlay(_)
    ));
    let Effect::Stroke(stroke) = &file.layers[5].effects.as_ref().unwrap().items[0] else {
        panic!("stroke");
    };
    assert_eq!(stroke.position, StrokePosition::Inside);
}

#[test]
fn the_drop_shadow_cell_darkens_below_and_right() {
    let region = region();
    let backdrop = sample_rgba8(&region, 5, 5)[0];
    let shadow = sample_rgba8(&region, 50, 52)[0];
    assert!(
        shadow < backdrop,
        "shadow {shadow} is darker than {backdrop}"
    );
}

#[test]
fn the_outer_glow_cell_lightens_outside_the_square() {
    let region = region();
    let backdrop = sample_rgba8(&region, 5, 5)[0];
    let glow = sample_rgba8(&region, 138, 32)[0];
    assert!(glow > backdrop, "glow {glow} is lighter than {backdrop}");
}

#[test]
fn the_inner_shadow_cell_darkens_inside_the_top_left_edge() {
    let region = region();
    let edge = sample_rgba8(&region, 82, 18);
    let center = sample_rgba8(&region, 96, 32);
    assert!(
        edge[0] < center[0],
        "edge {edge:?} is darker than {center:?}"
    );
}

#[test]
fn the_inner_glow_cell_lightens_inside_the_edge() {
    let region = region();
    let edge = sample_rgba8(&region, 18, 96);
    let center = sample_rgba8(&region, 32, 96);
    assert!(
        edge[0] > center[0],
        "edge {edge:?} is lighter than {center:?}"
    );
}

#[test]
fn the_stroke_cell_paints_an_inside_white_band() {
    let region = region();
    let band = sample_rgba8(&region, 82, 96);
    assert!(band[0] > 200 && band[1] > 200 && band[2] > 200, "{band:?}");
    let center = sample_rgba8(&region, 96, 96);
    assert!(center[2] > 180, "the layer color stays blue: {center:?}");
}

#[test]
fn the_bevel_and_satin_cells_change_the_layer_edge() {
    let region = region();
    let beveled = [146, 148, 172, 174]
        .iter()
        .any(|x| sample_rgba8(&region, *x, 96)[0] != 140);
    assert!(beveled, "the bevel paints at least one edge pixel");
    let satined = (17..47).any(|x| sample_rgba8(&region, x, 160)[0] != 120);
    assert!(satined, "the satin cell changes at least one pixel");
}

#[test]
fn the_overlay_cells_replace_and_ramp_the_layer_color() {
    let region = region();
    let red = sample_rgba8(&region, 96, 160);
    assert!(red[0] > 150, "the red overlay raises red: {red:?}");
    assert!(red[1] < 80, "the red overlay lowers green: {red:?}");
    let left = sample_rgba8(&region, 146, 160)[0];
    let right = sample_rgba8(&region, 174, 160)[0];
    assert!(
        left < right,
        "gradient left {left} is darker than right {right}"
    );
}

#[test]
fn the_fixture_composites_without_tile_seams() {
    let document = PsdDocument::open(&common::fixture("layer_effects.psd")).expect("fixture opens");
    let full = document
        .composite_region(Viewport::full(192, 192), &ParseOptions::none())
        .expect("full composite");
    let mut tiles = Vec::new();
    for (left, top) in [(0, 0), (96, 0), (0, 96), (96, 96)] {
        tiles.push(
            document
                .composite_region(
                    Viewport::new(left, top, left + 96, top + 96),
                    &ParseOptions::none(),
                )
                .expect("tile composite"),
        );
    }
    let full_pixels = full.rgba8();
    let tile_pixels: Vec<Vec<u8>> = tiles
        .iter()
        .map(tarragon_psd::CompositeRegion::rgba8)
        .collect();
    for y in 0..192u32 {
        for x in 0..192u32 {
            let tile = (usize::from(x >= 96)) + 2 * usize::from(y >= 96);
            let local_x = usize::try_from(x % 96).expect("x");
            let local_y = usize::try_from(y % 96).expect("y");
            let full_index =
                (usize::try_from(y).expect("y") * 192 + usize::try_from(x).expect("x")) * 4;
            let tile_index = (local_y * 96 + local_x) * 4;
            assert_eq!(
                &tile_pixels[tile][tile_index..tile_index + 4],
                &full_pixels[full_index..full_index + 4],
                "pixel ({x}, {y}) differs across tiles"
            );
        }
    }
}
