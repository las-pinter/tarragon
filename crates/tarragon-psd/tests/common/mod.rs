//! Shared helpers for the PSD test binaries.
//!
//! The fixture helpers read the committed corpus in `tests/fixtures/psd/`. The
//! synthetic builder writes small PSD files in memory. The encoders build the
//! descriptor and adjustment blocks that those files carry.
//!
//! Each test binary uses only part of this module.
#![allow(dead_code)]

mod builder;
mod effects;
mod encoders;

// Each test binary uses only part of this module, so the re-exports can look
// unused to one binary even when another uses them.
#[allow(unused_imports)]
pub use builder::{
    DocumentSpec, LayerSpec, MaskSpec, build_psd, layer_extra, layer_record, push_i32,
    push_samples, push_u16, push_u32, sample_bytes,
};
#[allow(unused_imports)]
pub use effects::{
    bevel_effect, bool_item, color_item, common, glow_effect, gradient_overlay_effect,
    gradient_stroke_effect, legacy_lrfx, legacy_shadow_body, legacy_shadow_body_with_light, lfx2,
    lfx2_disabled, linear_contour, satin_effect, set_bool_item, set_enum_item, shadow_effect,
    solid_overlay_effect, square_document_with_fill_opacity, square_document_with_opacity,
    stroke_effect,
};
#[allow(unused_imports)]
pub use encoders::{
    brit_block, curves_block, descriptor_body, descriptor_item, gdfl_black_to_white,
    gdfl_black_to_white_midpoint, identity_levels, levels_block, levels_record, push_id,
    push_unicode, rgbc, soco, value_boolean, value_descriptor, value_double, value_enum,
    value_integer, value_list, value_unit_float, versioned,
};

use std::path::{Path, PathBuf};

use tarragon_psd::composite::CompositeRegion;
use tarragon_psd::{ParseOptions, PsdDocument, Viewport};

/// The absolute path of a committed fixture.
pub fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/psd")
        .join(name)
}

/// The per-channel tolerance for golden comparisons.
///
/// The value is provisional. A small per-channel band absorbs rounding through
/// float compositing. The measured maximum replaces it once the first golden
/// set lands.
pub const GOLDEN_TOLERANCE: u8 = 2;

/// The manifest of golden stems. The Python generator reads the same file.
const GOLDEN_MANIFEST: &str = include_str!("../../../../tests/fixtures/psd/golden/manifest.txt");

/// The golden stems from the manifest, in file order.
pub fn golden_stems() -> Vec<&'static str> {
    GOLDEN_MANIFEST
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .collect()
}

/// The corpus file name for a golden stem. The one PSB fixture ends in `.psb`.
///
/// # Panics
///
/// Panics when neither the `.psd` nor the `.psb` candidate exists.
pub fn fixture_for(stem: &str) -> String {
    let psd = format!("{stem}.psd");
    if fixture(&psd).exists() {
        return psd;
    }
    let psb = format!("{stem}.psb");
    if fixture(&psb).exists() {
        return psb;
    }
    panic!(
        "{stem}: neither {} nor {} exists",
        fixture(&psd).display(),
        fixture(&psb).display()
    );
}

/// The absolute path of a committed golden image.
pub fn golden_path(name: &str) -> PathBuf {
    fixture("golden").join(format!("{name}.png"))
}

/// Load a committed golden image, or `None` when the file is absent.
///
/// # Panics
///
/// Panics when the file decodes to a color type other than 8-bit RGBA. A
/// silently converted golden would hide a generator change.
pub fn load_golden(name: &str) -> Option<image::RgbaImage> {
    let path = golden_path(name);
    if !path.exists() {
        return None;
    }
    let image = image::open(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    assert_eq!(
        image.color(),
        image::ColorType::Rgba8,
        "{}: the golden must decode as RGBA8",
        path.display()
    );
    Some(image.to_rgba8())
}

/// Compare composited RGB bytes with a golden image.
///
/// The compositor stores straight color, so each pixel compares by its R, G,
/// and B channels and alpha is not compared. The comparison prints the observed
/// maximum color-channel delta when `TARRAGON_GOLDEN_REPORT` is set. The same
/// value is always in the failure message.
///
/// # Panics
///
/// Panics when the sizes differ or any pixel exceeds `tolerance` on a color
/// channel.
pub fn assert_matches_golden(
    composited: &[u8],
    width: u32,
    height: u32,
    golden: &image::RgbaImage,
    tolerance: u8,
    label: &str,
) {
    let (golden_width, golden_height) = golden.dimensions();
    assert_eq!(
        (width, height),
        (golden_width, golden_height),
        "{label}: size {width}x{height} does not match the golden size {golden_width}x{golden_height}"
    );

    assert!(width > 0 && height > 0, "{label}: empty region");

    let expected = golden.as_raw();
    assert_eq!(
        composited.len(),
        expected.len(),
        "{label}: composited {} bytes, golden {} bytes",
        composited.len(),
        expected.len()
    );

    let row_bytes = usize::try_from(width).expect("width") * 4;

    let mut max_delta = 0u8;
    let mut over = 0usize;
    let mut first_over = None;

    for (row, (got_row, want_row)) in composited
        .chunks_exact(row_bytes)
        .zip(expected.chunks_exact(row_bytes))
        .enumerate()
    {
        for (column, (got, want)) in got_row
            .as_chunks::<4>()
            .0
            .iter()
            .zip(want_row.as_chunks::<4>().0.iter())
            .enumerate()
        {
            let got_color = [got[0], got[1], got[2]];
            let want_color = [want[0], want[1], want[2]];
            let mut pixel_over = false;
            for (&got_channel, &want_channel) in got_color.iter().zip(want_color.iter()) {
                let delta = got_channel.abs_diff(want_channel);
                max_delta = max_delta.max(delta);
                pixel_over = pixel_over || delta > tolerance;
            }
            if pixel_over {
                over += 1;
                if first_over.is_none() {
                    let x = u32::try_from(column).expect("column");
                    let y = u32::try_from(row).expect("row");
                    first_over = Some((x, y, got_color, want_color));
                }
            }
        }
    }

    if std::env::var_os("TARRAGON_GOLDEN_REPORT").is_some() {
        println!("{label}: observed max color-channel delta {max_delta} (tolerance {tolerance})");
    }

    if over > 0 {
        let (x, y, got_pixel, want_pixel) = first_over.expect("a failing pixel");
        panic!(
            "{label}: {over} pixels over tolerance {tolerance}; max delta {max_delta}; first at ({x}, {y}) is {got_pixel:?}, golden is {want_pixel:?}"
        );
    }
}

/// Composite a fixture over its full canvas.
pub fn composite_fixture(name: &str) -> CompositeRegion {
    let document = PsdDocument::open(&fixture(name)).unwrap_or_else(|e| panic!("{name}: {e}"));
    let viewport = Viewport::full(document.file().header.width, document.file().header.height);
    document
        .composite_region(viewport, &ParseOptions::none())
        .unwrap_or_else(|e| panic!("{name}: {e}"))
}

/// Composite owned PSD bytes over one viewport.
pub fn composite(bytes: Vec<u8>, viewport: Viewport) -> CompositeRegion {
    let document = PsdDocument::from_bytes(bytes, &ParseOptions::none()).expect("the file parses");
    document
        .composite_region(viewport, &ParseOptions::none())
        .expect("the file composites")
}

/// Composite owned PSD bytes over the full canvas.
pub fn composite_bytes(bytes: Vec<u8>) -> CompositeRegion {
    let document =
        PsdDocument::from_bytes(bytes, &ParseOptions::none()).expect("the synthetic file parses");
    let viewport = Viewport::full(document.file().header.width, document.file().header.height);
    document
        .composite_region(viewport, &ParseOptions::none())
        .expect("the synthetic file composites")
}

/// The 8-bit or 16-bit sample for an 8-bit value.
pub fn scaled(value: u16, depth: u16) -> u16 {
    if depth == 8 { value } else { value * 257 }
}

/// Read one output pixel as RGBA8.
pub fn sample_rgba8(region: &CompositeRegion, x: u32, y: u32) -> [u8; 4] {
    let pixels = region.rgba8();
    let index = usize::try_from(y).expect("y") * usize::try_from(region.width()).expect("width")
        + usize::try_from(x).expect("x");
    [
        pixels[index * 4],
        pixels[index * 4 + 1],
        pixels[index * 4 + 2],
        pixels[index * 4 + 3],
    ]
}
