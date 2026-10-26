//! PSD and PSB renderer behavior.
//!
//! The tests prove the merged-first rule: a usable merged image wins over the
//! layers, and a missing, truncated, or unsupported merged image falls back to
//! the layer compositor. They also prove the stepped read for a canvas over
//! 8192 pixels, the 8192 pixel refusal on the compositor path, the dispatch of
//! both extensions, and the error mapping.
//!
//! The files are built in a temporary folder with the builder of the parser
//! crate, or read from the committed corpus in `tests/fixtures/psd/`.

#[path = "../../tarragon-psd/tests/common/mod.rs"]
mod psd_builder;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use psd_builder::{DocumentSpec, LayerSpec, build_psd, push_u16, rle_merged_section};
use tarragon_core::error::RenderError;
use tarragon_psd::{Deadline, ParseOptions, PsdDocument};
use tarragon_render::psd::render_psd_with;
use tarragon_render::registry;

/// A temporary folder that removes itself on drop.
struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(tag: &str) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "tarragon-render-psd-{tag}-{}-{id}",
            std::process::id()
        ));
        std::fs::create_dir_all(&path).expect("create the temp folder");
        Self { path }
    }

    /// Write `bytes` to `name` in the folder and return the path.
    fn write(&self, name: &str, bytes: &[u8]) -> PathBuf {
        let path = self.path.join(name);
        std::fs::write(&path, bytes).expect("write the file");
        path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// The absolute path of a committed fixture.
fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/psd")
        .join(name)
}

/// An 8-bit RGB document with one red layer and a flat merged image of
/// `merged`, or no merged section at all when `merged` is `None`.
fn red_layer_document(merged: Option<[u8; 3]>) -> Vec<u8> {
    let mut spec = DocumentSpec::new(16, 16, 8, 3);
    spec.layers
        .push(LayerSpec::opaque((0, 0, 16, 16), [200, 0, 0]));
    match merged {
        Some(color) => {
            spec.merged = color
                .iter()
                .map(|value| vec![u16::from(*value); 256])
                .collect();
        }
        None => spec.merged_section = Some(Vec::new()),
    }
    build_psd(&spec)
}

/// The RGBA bytes of the pixel at `(x, y)` of a rendered image.
fn pixel(image: &image::DynamicImage, x: u32, y: u32) -> [u8; 4] {
    image.to_rgba8().get_pixel(x, y).0
}

#[test]
fn a_usable_merged_image_wins_over_the_layers() {
    let folder = TempDir::new("merged-wins");
    // The layer is red and the merged image is blue, so the pixel tells which
    // path ran.
    let path = folder.write("a.psd", &red_layer_document(Some([0, 0, 200])));
    let image = registry::render(&path).expect("renders");
    assert_eq!((image.width(), image.height()), (16, 16));
    assert_eq!(pixel(&image, 8, 8), [0, 0, 200, 255]);
}

#[test]
fn a_blank_merged_image_of_the_right_size_is_still_used() {
    let folder = TempDir::new("blank");
    let path = folder.write("a.psd", &red_layer_document(Some([0, 0, 0])));
    let image = registry::render(&path).expect("renders");
    assert_eq!(pixel(&image, 3, 3), [0, 0, 0, 255]);
}

#[test]
fn a_missing_merged_image_uses_the_layers() {
    let folder = TempDir::new("absent");
    let path = folder.write("a.psd", &red_layer_document(None));
    let image = registry::render(&path).expect("renders");
    assert_eq!(pixel(&image, 8, 8), [200, 0, 0, 255]);
}

#[test]
fn a_merged_image_with_an_unsupported_compression_uses_the_layers() {
    let folder = TempDir::new("zip");
    let mut spec = DocumentSpec::new(16, 16, 8, 3);
    spec.layers
        .push(LayerSpec::opaque((0, 0, 16, 16), [200, 0, 0]));
    let mut section = Vec::new();
    push_u16(&mut section, 2);
    section.extend_from_slice(&[7; 40]);
    spec.merged_section = Some(section);
    let path = folder.write("a.psd", &build_psd(&spec));
    let image = registry::render(&path).expect("renders");
    assert_eq!(pixel(&image, 8, 8), [200, 0, 0, 255]);
}

#[test]
fn a_merged_image_cut_short_uses_the_layers() {
    let folder = TempDir::new("short");
    let mut bytes = red_layer_document(Some([0, 0, 200]));
    // Cut inside the raw merged planes. The layers sit before them.
    bytes.truncate(bytes.len() - 100);
    let path = folder.write("a.psd", &bytes);
    let image = registry::render(&path).expect("renders");
    assert_eq!(pixel(&image, 8, 8), [200, 0, 0, 255]);
}

#[test]
fn a_wide_canvas_reads_the_merged_image_through_a_step() {
    let folder = TempDir::new("wide");
    let (width, height) = (8200usize, 6usize);
    let mut spec = DocumentSpec::new(8200, 6, 8, 1);
    spec.merged_section = Some(rle_merged_section(
        width,
        &[vec![77u8; width * height]],
        false,
    ));
    let path = folder.write("wide.psd", &build_psd(&spec));
    let image = registry::render(&path).expect("renders");
    assert_eq!((image.width(), image.height()), (4100, 3));
    assert_eq!(pixel(&image, 4099, 2), [77, 77, 77, 255]);
}

#[test]
fn a_tall_psb_canvas_reads_the_merged_image_through_a_step() {
    let folder = TempDir::new("tall");
    let (width, height) = (4usize, 16_500usize);
    let mut spec = DocumentSpec::new(4, 16_500, 8, 1);
    spec.psb = true;
    spec.merged_section = Some(rle_merged_section(
        width,
        &[vec![33u8; width * height]],
        true,
    ));
    let path = folder.write("tall.psb", &build_psd(&spec));
    let image = registry::render(&path).expect("renders");
    // The step is 3, so 16500 rows keep 5500 and 4 columns keep 2.
    assert_eq!((image.width(), image.height()), (2, 5500));
    assert_eq!(pixel(&image, 1, 5499), [33, 33, 33, 255]);
}

#[test]
fn a_canvas_over_the_cap_with_no_merged_image_is_too_large() {
    let folder = TempDir::new("too-large");
    let mut spec = DocumentSpec::new(8200, 2, 8, 1);
    spec.layers.push(LayerSpec::gray((0, 0, 2, 8200), 90));
    spec.merged_section = Some(Vec::new());
    let path = folder.write("big.psd", &build_psd(&spec));
    match registry::render(&path) {
        Err(RenderError::TooLarge { edge, cap }) => assert_eq!((edge, cap), (8200, 8192)),
        other => panic!("expected TooLarge, got {other:?}"),
    }
}

#[test]
fn the_compositor_path_matches_a_direct_composite() {
    let folder = TempDir::new("direct");
    let path = folder.write("a.psd", &red_layer_document(None));
    let document =
        PsdDocument::open_lenient(&path, &tarragon_psd::ParseOptions::none()).expect("opens");
    let direct = document
        .composite_region(
            tarragon_psd::Viewport::full(16, 16),
            &tarragon_psd::ParseOptions::none(),
        )
        .expect("composites")
        .rgba8();
    let image = registry::render(&path).expect("renders");
    assert_eq!(image.to_rgba8().into_raw(), direct);
}

#[test]
fn both_extensions_render_through_the_registry() {
    for (name, edge) in [("rgb_8bit.psd", 64), ("psb_8bit.psb", 64)] {
        let image = registry::render(&fixture(name)).expect(name);
        assert_eq!((image.width(), image.height()), (edge, edge), "{name}");
    }
    let upper = TempDir::new("upper");
    let copy = upper.write(
        "SHOUT.PSD",
        &std::fs::read(fixture("rgb_8bit.psd")).expect("read"),
    );
    assert!(registry::render(&copy).is_ok());
}

#[test]
fn every_committed_fixture_renders_at_its_canvas_size() {
    for (name, edge) in [
        ("rgb_16bit.psd", 48),
        ("cmyk_8bit.psd", 64),
        ("layered_blend_modes.psd", 96),
        ("layer_groups.psd", 96),
        ("clipping_mask.psd", 96),
        ("layer_mask.psd", 64),
        ("layer_effects.psd", 192),
    ] {
        let image = registry::render(&fixture(name)).expect(name);
        assert_eq!((image.width(), image.height()), (edge, edge), "{name}");
    }
}

#[test]
fn a_missing_file_maps_to_io() {
    let folder = TempDir::new("missing");
    let result = registry::render(&folder.path.join("none.psd"));
    assert!(matches!(result, Err(RenderError::Io { .. })), "{result:?}");
}

#[test]
fn malformed_bytes_map_to_decode() {
    let folder = TempDir::new("junk");
    let junk = folder.write("junk.psd", b"this is not a photoshop file at all");
    assert!(matches!(
        registry::render(&junk),
        Err(RenderError::Decode { .. })
    ));
    let empty = folder.write("empty.psb", b"");
    assert!(matches!(
        registry::render(&empty),
        Err(RenderError::Decode { .. })
    ));
    let result = registry::render(&fixture("truncated.psd"));
    assert!(
        matches!(result, Err(RenderError::Decode { .. })),
        "{result:?}"
    );
}

#[test]
fn an_unsupported_color_mode_maps_to_decode() {
    let folder = TempDir::new("lab");
    // Lab color has no merged or layer conversion.
    let mut spec = DocumentSpec::new(4, 4, 8, 9);
    spec.merged_section = Some(rle_merged_section(
        4,
        &[vec![1; 16], vec![1; 16], vec![1; 16]],
        false,
    ));
    let path = folder.write("lab.psd", &build_psd(&spec));
    let result = registry::render(&path);
    assert!(
        matches!(result, Err(RenderError::Decode { .. })),
        "{result:?}"
    );
}

/// A document with a red layer and a ZIP merged section, which only the
/// lenient open accepts.
fn bad_merged_with_layers() -> Vec<u8> {
    let mut spec = DocumentSpec::new(16, 16, 8, 3);
    spec.layers
        .push(LayerSpec::opaque((0, 0, 16, 16), [200, 0, 0]));
    let mut section = Vec::new();
    push_u16(&mut section, 2);
    section.extend_from_slice(&[7; 40]);
    spec.merged_section = Some(section);
    build_psd(&spec)
}

#[test]
fn an_expired_deadline_is_a_timeout_and_not_a_layer_render() {
    let folder = TempDir::new("deadline");
    let path = folder.write("a.psd", &bad_merged_with_layers());
    // With default options the layers draw, so the file is renderable.
    assert!(render_psd_with(&path, &ParseOptions::default()).is_ok());
    let expired = ParseOptions::with_deadline(Deadline::after(Duration::ZERO));
    let result = render_psd_with(&path, &expired);
    assert!(
        matches!(result, Err(RenderError::Timeout { .. })),
        "{result:?}"
    );
}

#[test]
fn a_set_cancel_flag_is_cancelled_and_not_a_layer_render() {
    let folder = TempDir::new("cancel");
    let path = folder.write("a.psd", &bad_merged_with_layers());
    let options = ParseOptions::none().with_cancel(Arc::new(AtomicBool::new(true)));
    let result = render_psd_with(&path, &options);
    assert!(
        matches!(result, Err(RenderError::Cancelled { .. })),
        "{result:?}"
    );
}

#[test]
fn a_merged_image_with_too_few_channels_uses_the_layers() {
    let folder = TempDir::new("few");
    let mut spec = DocumentSpec::new(16, 16, 8, 3);
    spec.layers
        .push(LayerSpec::opaque((0, 0, 16, 16), [200, 0, 0]));
    // Two merged channels in an RGB file: no blue plane.
    spec.merged_channels = Some(2);
    spec.merged = vec![vec![0u16; 256]; 2];
    let path = folder.write("a.psd", &build_psd(&spec));
    let image = registry::render(&path).expect("renders");
    assert_eq!(pixel(&image, 8, 8), [200, 0, 0, 255]);
}
