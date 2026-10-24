//! Golden image tests for the PSD compositor.
//!
//! The golden PNG files are committed under `tests/fixtures/psd/golden/`, and
//! `golden/SOURCES.md` records how they were made. A missing golden fails the
//! test.
//!
//! The comparison uses straight RGB and ignores alpha, because the goldens
//! composite onto an opaque canvas.

mod common;

use common::{
    GOLDEN_TOLERANCE, assert_matches_golden, composite_fixture, fixture, fixture_for, golden_path,
    golden_stems, load_golden,
};

/// Composite one fixture over its full canvas and compare it with its golden.
fn check(stem: &str) {
    let region = composite_fixture(&fixture_for(stem));
    let Some(golden) = load_golden(stem) else {
        panic!("missing golden: {}", golden_path(stem).display());
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
fn the_rgb_8bit_fixture_matches_its_golden() {
    check("rgb_8bit");
}

#[test]
fn the_rgb_16bit_fixture_matches_its_golden() {
    check("rgb_16bit");
}

#[test]
fn the_cmyk_8bit_fixture_matches_its_golden() {
    check("cmyk_8bit");
}

#[test]
fn the_layered_blend_modes_fixture_matches_its_golden() {
    check("layered_blend_modes");
}

#[test]
fn the_layer_groups_fixture_matches_its_golden() {
    check("layer_groups");
}

#[test]
fn the_clipping_mask_fixture_matches_its_golden() {
    check("clipping_mask");
}

#[test]
fn the_layer_mask_fixture_matches_its_golden() {
    check("layer_mask");
}

#[test]
fn the_psb_8bit_fixture_matches_its_golden() {
    check("psb_8bit");
}

#[test]
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
