//! Shared helpers for the PSD test binaries.
//!
//! The fixture helpers read the committed corpus in `tests/fixtures/psd/`. The
//! synthetic builder writes small PSD files in memory. The encoders build the
//! descriptor and adjustment blocks that those files carry.
//!
//! Each test binary uses only part of this module.
#![allow(dead_code)]

mod builder;
mod encoders;

// Each test binary uses only part of this module, so the re-exports can look
// unused to one binary even when another uses them.
#[allow(unused_imports)]
pub use builder::{
    DocumentSpec, LayerSpec, MaskSpec, build_psd, layer_extra, layer_record, push_i32,
    push_samples, push_u16, push_u32, sample_bytes,
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

/// Composite a fixture over its full canvas.
pub fn composite_fixture(name: &str) -> CompositeRegion {
    let document = PsdDocument::open(&fixture(name)).unwrap_or_else(|e| panic!("{name}: {e}"));
    let viewport = Viewport::full(document.file().header.width, document.file().header.height);
    document
        .composite_region(viewport, &ParseOptions::none())
        .unwrap_or_else(|e| panic!("{name}: {e}"))
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
