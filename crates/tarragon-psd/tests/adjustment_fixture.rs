//! Hand-built adjustment-block fixture.
//!
//! The committed `adjustment_blocks.psd` holds ten adjustment layers, one per
//! block kind, each limited to its own cell by a layer mask. The Rust builder
//! owns the file. psd-tools skips every adjustment layer when it composites, so
//! no Photoshop render can serve as a golden image for these blocks.
//!
//! The check test keeps the committed bytes and the builder in step. The
//! ignored test rewrites the file after a builder change:
//!
//! ```text
//! cargo test -p tarragon-psd --test adjustment_fixture -- --ignored
//! ```

mod common;

use common::{
    DEFAULT_WEIGHTS, DocumentSpec, LayerSpec, NO_EDITS, black_to_white, blnc_block, blwh,
    build_psd, clrl_block, cube, expa_block, grdm, hue2_block, mixr_block, phfl_block, selc_block,
    viba,
};
use tarragon_psd::{LayerContent, ParseOptions, PsdDocument};

/// The size limit for a committed fixture, in bytes.
const SIZE_LIMIT: u64 = 5 * 1024 * 1024;

/// One fixture cell: the block key, the backdrop color, and the block bytes.
type Cell = ([u8; 4], [u16; 3], Vec<u8>);

/// The ten cells, in grid order: five columns, two rows.
fn cells() -> Vec<Cell> {
    let mut records = [[0i16; 4]; 9];
    records[0] = [-50, 0, 0, 0];
    vec![
        (*b"expA", [100, 100, 100], expa_block(1.0, 0.0, 1.0)),
        (*b"vibA", [200, 100, 60], viba(None, Some(-100))),
        (*b"blwh", [255, 0, 0], blwh(DEFAULT_WEIGHTS, None)),
        (*b"grdm", [64, 64, 64], grdm(true, &black_to_white())),
        (
            *b"clrL",
            [200, 100, 20],
            clrl_block(
                b"3DLUT",
                b"LUTFormatCUBE",
                Some(&cube(|c| [c[2], c[1], c[0]])),
                false,
            ),
        ),
        (
            *b"hue2",
            [200, 50, 50],
            hue2_block(None, [180, 0, 0], NO_EDITS),
        ),
        (
            *b"blnc",
            [40, 40, 40],
            blnc_block([[100, 0, 0], [0; 3], [0; 3]], false),
        ),
        (
            *b"mixr",
            [50, 150, 200],
            mixr_block(false, [[0, 0, 100, 0], [0, 100, 0, 0], [100, 0, 0, 10]]),
        ),
        (*b"selc", [200, 60, 60], selc_block(true, records)),
        (
            *b"phfl",
            [128, 128, 128],
            phfl_block([236, 138, 0], 100, false),
        ),
    ]
}

/// Build the adjustment fixture bytes.
fn adjustment_bytes() -> Vec<u8> {
    let mut spec = DocumentSpec::new(104, 44, 8, 3);
    let mut base = LayerSpec::opaque((0, 0, 44, 104), [128, 128, 128]);
    base.name = Some("Base".to_string());
    spec.layers.push(base);
    for (index, (key, backdrop, block)) in cells().into_iter().enumerate() {
        let column = i32::try_from(index % 5).expect("column");
        let row = i32::try_from(index / 5).expect("row");
        let top = 4 + 20 * row;
        let left = 4 + 20 * column;
        let rect = (top, left, top + 16, left + 16);
        let mut patch = LayerSpec::opaque(rect, backdrop);
        patch.name = Some(format!("Patch {index}"));
        spec.layers.push(patch);
        // The mask limits the block to its cell. Channel -2 is the user mask.
        let mut adjustment = LayerSpec::content((0, 0, 44, 104))
            .with_block(key, block)
            .with_mask(rect, 0);
        adjustment.channels = vec![(-2, vec![255; 256])];
        adjustment.name = Some(format!("Adjustment {index}"));
        spec.layers.push(adjustment);
    }
    build_psd(&spec)
}

#[test]
fn the_committed_adjustment_fixture_matches_the_builder() {
    let committed = std::fs::read(common::fixture("adjustment_blocks.psd")).expect("fixture reads");
    assert_eq!(
        committed,
        adjustment_bytes(),
        "regenerate the fixture after a builder change"
    );
}

#[test]
fn the_adjustment_fixture_is_small_and_fully_supported() {
    let path = common::fixture("adjustment_blocks.psd");
    let size = std::fs::metadata(&path).expect("fixture exists").len();
    assert!(size < SIZE_LIMIT, "fixture is {size} bytes");
    let bytes = std::fs::read(&path).expect("fixture reads");
    let document = PsdDocument::from_bytes(bytes, &ParseOptions::none()).expect("parses");
    let layers = &document.file().layers;
    let adjustments = layers
        .iter()
        .filter(|layer| matches!(layer.content, LayerContent::Adjustment(_)))
        .count();
    let unsupported = layers
        .iter()
        .filter(|layer| matches!(layer.content, LayerContent::Unsupported))
        .count();
    assert_eq!(adjustments, 10);
    assert_eq!(unsupported, 0);
}

#[test]
#[ignore = "rewrites the committed adjustment fixture"]
fn regenerate_adjustment_fixture() {
    std::fs::write(common::fixture("adjustment_blocks.psd"), adjustment_bytes())
        .expect("fixture writes");
}
