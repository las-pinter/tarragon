//! Hand-built fill-layer fixture.
//!
//! The committed `fill_layers.psd` holds four fill layers, one per horizontal
//! band: two solid colors (one at half opacity), a gradient, and a pattern.
//! Each layer has no pixels of its own and a layer mask limits it to its band.
//! The Rust builder owns the file. psd-tools renders all three fill kinds, so
//! its render can serve as a golden image.
//!
//! The check test keeps the committed bytes and the builder in step. The
//! ignored test rewrites the file after a builder change:
//!
//! ```text
//! cargo test -p tarragon-psd --test fill_fixture -- --ignored
//! ```

mod common;

use common::{
    DocumentSpec, LayerSpec, PtflSpec, build_psd, gdfl_black_to_white, pattern_record, ptfl_block,
    soco, tagged_block, tile4,
};
use tarragon_psd::{LayerContent, ParseOptions, PsdDocument};

/// The size limit for a committed fixture, in bytes.
const SIZE_LIMIT: u64 = 5 * 1024 * 1024;

/// One fill band: the block key, the block bytes, and the layer opacity.
type Band = ([u8; 4], Vec<u8>, u8);

/// The four bands, top to bottom.
fn bands() -> Vec<Band> {
    vec![
        (*b"SoCo", soco(200.0, 60.0, 60.0), 255),
        (*b"SoCo", soco(60.0, 120.0, 200.0), 128),
        (*b"GdFl", gdfl_black_to_white(0.0), 255),
        (*b"PtFl", ptfl_block(&PtflSpec::new("tile")), 255),
    ]
}

/// Build the fill fixture bytes.
fn fill_bytes() -> Vec<u8> {
    let mut spec = DocumentSpec::new(64, 64, 8, 3);
    let mut base = LayerSpec::opaque((0, 0, 64, 64), [230, 230, 230]);
    base.name = Some("Base".to_string());
    spec.layers.push(base);
    for (index, (key, block, opacity)) in bands().into_iter().enumerate() {
        let top = 16 * i32::try_from(index).expect("band");
        let rect = (top, 0, top + 16, 64);
        // The mask limits the fill to its band. Channel -2 is the user mask.
        let mut fill = LayerSpec::content((0, 0, 64, 64))
            .with_block(key, block)
            .with_mask(rect, 0);
        fill.channels = vec![(-2, vec![255; 1024])];
        fill.opacity = opacity;
        fill.name = Some(format!("Fill {index}"));
        spec.layers.push(fill);
    }
    spec.extra_global_blocks = tagged_block(*b"Patt", &pattern_record(&tile4("tile")));
    build_psd(&spec)
}

#[test]
fn the_committed_fill_fixture_matches_the_builder() {
    let committed = std::fs::read(common::fixture("fill_layers.psd")).expect("fixture reads");
    assert_eq!(
        committed,
        fill_bytes(),
        "regenerate the fixture after a builder change"
    );
}

#[test]
fn the_fill_fixture_is_small_and_fully_supported() {
    let path = common::fixture("fill_layers.psd");
    let size = std::fs::metadata(&path).expect("fixture exists").len();
    assert!(size < SIZE_LIMIT, "fixture is {size} bytes");
    let bytes = std::fs::read(&path).expect("fixture reads");
    let document = PsdDocument::from_bytes(bytes, &ParseOptions::none()).expect("parses");
    let layers = &document.file().layers;
    assert_eq!(layers.len(), 5, "Base and four fill layers");
    let fills = &layers[1..];
    assert!(matches!(fills[0].content, LayerContent::SolidColor(_)));
    assert!(matches!(fills[1].content, LayerContent::SolidColor(_)));
    assert!(matches!(fills[2].content, LayerContent::Gradient(_)));
    assert!(matches!(fills[3].content, LayerContent::Pattern(_)));
    let unsupported = layers
        .iter()
        .filter(|layer| matches!(layer.content, LayerContent::Unsupported))
        .count();
    assert_eq!(unsupported, 0);
}

#[test]
#[ignore = "rewrites the committed fill fixture"]
fn regenerate_fill_fixture() {
    std::fs::write(common::fixture("fill_layers.psd"), fill_bytes()).expect("fixture writes");
}
