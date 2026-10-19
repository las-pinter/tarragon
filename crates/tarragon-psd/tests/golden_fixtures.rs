//! Golden image tests for the PSD compositor.
//!
//! The golden PNG files come from `tests/fixtures/psd/generate_goldens.py`,
//! which renders each fixture with psd-tools. CI writes the files before it
//! runs these tests with `--run-ignored ignored-only`.
//!
//! The comparison uses straight RGB and ignores alpha, because the oracle
//! composites onto an opaque canvas.

mod common;

use common::{
    GOLDEN_TOLERANCE, assert_matches_golden, composite_fixture, fixture, fixture_for, golden_path,
    golden_stems, load_golden,
};

/// Composite one fixture over its full canvas and compare it with its golden.
fn check(stem: &str) {
    let region = composite_fixture(&fixture_for(stem));
    let Some(golden) = load_golden(stem) else {
        println!("missing golden: {}", golden_path(stem).display());
        return;
    };
    assert_matches_golden(
        &region.rgba8(),
        region.width(),
        region.height(),
        &golden,
        GOLDEN_TOLERANCE,
        stem,
    );
}

#[test]
#[ignore = "CI generates the goldens before it runs these tests, and a local run is described in `golden/SOURCES.md`"]
fn the_rgb_8bit_fixture_matches_its_golden() {
    check("rgb_8bit");
}

#[test]
#[ignore = "CI generates the goldens before it runs these tests, and a local run is described in `golden/SOURCES.md`"]
fn the_rgb_16bit_fixture_matches_its_golden() {
    check("rgb_16bit");
}

#[test]
#[ignore = "CI generates the goldens before it runs these tests, and a local run is described in `golden/SOURCES.md`"]
fn the_cmyk_8bit_fixture_matches_its_golden() {
    check("cmyk_8bit");
}

#[test]
#[ignore = "CI generates the goldens before it runs these tests, and a local run is described in `golden/SOURCES.md`"]
fn the_layered_blend_modes_fixture_matches_its_golden() {
    check("layered_blend_modes");
}

#[test]
#[ignore = "CI generates the goldens before it runs these tests, and a local run is described in `golden/SOURCES.md`"]
fn the_layer_groups_fixture_matches_its_golden() {
    check("layer_groups");
}

#[test]
#[ignore = "CI generates the goldens before it runs these tests, and a local run is described in `golden/SOURCES.md`"]
fn the_clipping_mask_fixture_matches_its_golden() {
    check("clipping_mask");
}

#[test]
#[ignore = "CI generates the goldens before it runs these tests, and a local run is described in `golden/SOURCES.md`"]
fn the_layer_mask_fixture_matches_its_golden() {
    check("layer_mask");
}

#[test]
#[ignore = "CI generates the goldens before it runs these tests, and a local run is described in `golden/SOURCES.md`"]
fn the_psb_8bit_fixture_matches_its_golden() {
    check("psb_8bit");
}

// CI writes the golden bytes before it runs this test with
// `--run-ignored ignored-only`.
#[test]
#[ignore = "CI generates the goldens before it runs these tests, and a local run is described in `golden/SOURCES.md`"]
fn every_golden_in_the_set_exists() {
    let stems = golden_stems();
    assert!(!stems.is_empty(), "the golden manifest is empty");
    for stem in stems {
        let path = golden_path(stem);
        assert!(path.exists(), "missing golden: {}", path.display());
        let fixture_name = fixture_for(stem);
        assert!(
            fixture(&fixture_name).exists(),
            "missing fixture for {stem}: {fixture_name}"
        );
    }
}
