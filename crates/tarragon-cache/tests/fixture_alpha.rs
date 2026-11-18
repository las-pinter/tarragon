//! Check the tier format choice on real rendered files.
//!
//! The CLIP fixture holds a PNG preview with alpha 128 pixels, so its tiers
//! must be PNG with the alpha kept. An opaque image must stay JPEG.

use std::path::{Path, PathBuf};

use image::{DynamicImage, Rgb, RgbImage};
use tarragon_cache::store::{CacheStore, TierFormat};
use tarragon_core::tier::Tier;

const PNG_MAGIC: [u8; 4] = [0x89, b'P', b'N', b'G'];
const JPEG_MAGIC: [u8; 2] = [0xFF, 0xD8];

/// A folder that removes itself on drop.
struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "tarragon-cache-fixture-{tag}-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&path).expect("create the temp folder");
        Self(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn fixture(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures")
        .join(relative)
}

#[test]
fn the_clip_fixture_keeps_its_alpha_in_png_tiers() {
    let rendered = tarragon_render::registry::render(&fixture("clip/preview.clip"))
        .expect("the fixture renders");
    let temp = TempDir::new("alpha");
    let store = CacheStore::new(&temp.0);

    let entry = store
        .write_all(&rendered, "clips", "abcd1234", "preview.clip")
        .expect("write the tiers");

    assert_eq!(entry.format, TierFormat::Png);
    for tier in Tier::ALL {
        let file = entry.path(tier);
        assert_eq!(
            file.file_name().and_then(|name| name.to_str()),
            Some("preview.clip.png")
        );
        let bytes = std::fs::read(file).expect("read the tier");
        assert_eq!(bytes.get(..4), Some(PNG_MAGIC.as_slice()));
    }
    // Pixel (0, 0) has alpha 128 in the source. The 48x32 source fits every
    // tier, so the full tier keeps the pixel position.
    let decoded = image::open(&entry.full).expect("decode").to_rgba8();
    assert_eq!(decoded.get_pixel(0, 0).0[3], 128);
}

#[test]
fn an_opaque_rendered_image_stays_jpeg() {
    let dir = TempDir::new("opaque");
    let path = dir.0.join("flat.png");
    DynamicImage::ImageRgb8(RgbImage::from_pixel(64, 48, Rgb([200, 100, 50])))
        .save(&path)
        .expect("write the source");
    let rendered = tarragon_render::registry::render(&path).expect("the source renders");
    let temp = TempDir::new("opaque-cache");
    let store = CacheStore::new(&temp.0);

    let entry = store
        .write_all(&rendered, "flats", "abcd1234", "flat.png")
        .expect("write the tiers");

    assert_eq!(entry.format, TierFormat::Jpeg);
    for tier in Tier::ALL {
        let file = entry.path(tier);
        assert_eq!(
            file.file_name().and_then(|name| name.to_str()),
            Some("flat.png.jpg")
        );
        let bytes = std::fs::read(file).expect("read the tier");
        assert_eq!(bytes.get(..2), Some(JPEG_MAGIC.as_slice()));
    }
}
