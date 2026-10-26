//! Tests for the per-source choice between PNG and JPEG tier files.

use image::{DynamicImage, GrayAlphaImage, GrayImage, ImageBuffer, LumaA, Rgb, RgbImage, Rgba};
use tarragon_core::tier::Tier;

use super::{CacheStore, FORMAT_MARKER, SourceStamp, TempDir, decoded_size, longest, source};
use crate::store::TierFormat;

const PNG_MAGIC: [u8; 4] = [0x89, b'P', b'N', b'G'];
const JPEG_MAGIC: [u8; 2] = [0xFF, 0xD8];

/// Write `image` as `name` and return the chosen format.
fn format_of(image: &DynamicImage, name: &str) -> TierFormat {
    let temp = TempDir::new("format-of");
    CacheStore::new(temp.path())
        .write_all(image, "gallery", "abcd1234", name)
        .expect("write the tiers")
        .format
}

#[test]
fn an_opaque_rgba_source_stays_jpeg() {
    let temp = TempDir::new("opaque-rgba");
    let store = CacheStore::new(temp.path());
    let entry = store
        .write_all(&source(300, 200), "gallery", "abcd1234", "art.png")
        .expect("write the tiers");
    assert_eq!(entry.format, TierFormat::Jpeg);
    for tier in Tier::ALL {
        let bytes = std::fs::read(entry.path(tier)).expect("read the tier");
        assert_eq!(bytes.get(..2), Some(JPEG_MAGIC.as_slice()));
    }
}

#[test]
fn one_pixel_with_alpha_254_gives_png() {
    let mut image = source(20, 20).to_rgba8();
    image.put_pixel(7, 9, Rgba([1, 2, 3, 254]));
    let temp = TempDir::new("alpha-254");
    let entry = CacheStore::new(temp.path())
        .write_all(
            &DynamicImage::ImageRgba8(image),
            "gallery",
            "abcd1234",
            "a.png",
        )
        .expect("write the tiers");
    assert_eq!(entry.format, TierFormat::Png);
    let bytes = std::fs::read(&entry.full).expect("read the tier");
    assert_eq!(bytes.get(..4), Some(PNG_MAGIC.as_slice()));
    let decoded = image::open(&entry.full).expect("decode").to_rgba8();
    assert_eq!(decoded.get_pixel(7, 9).0[3], 254);
}

#[test]
fn luma_luma_alpha_and_rgb_sources_pick_the_right_format() {
    let luma = DynamicImage::ImageLuma8(GrayImage::new(8, 8));
    assert_eq!(format_of(&luma, "l.png"), TierFormat::Jpeg);
    let rgb = DynamicImage::ImageRgb8(RgbImage::from_pixel(8, 8, Rgb([5, 6, 7])));
    assert_eq!(format_of(&rgb, "c.png"), TierFormat::Jpeg);
    let la_opaque = GrayAlphaImage::from_pixel(8, 8, LumaA([5, 255]));
    assert_eq!(
        format_of(&DynamicImage::ImageLumaA8(la_opaque), "la.png"),
        TierFormat::Jpeg
    );
    let la = GrayAlphaImage::from_pixel(8, 8, LumaA([5, 10]));
    assert_eq!(
        format_of(&DynamicImage::ImageLumaA8(la), "la.png"),
        TierFormat::Png
    );
}

#[test]
fn a_16_bit_alpha_below_the_maximum_gives_png() {
    let mut wide: ImageBuffer<Rgba<u16>, Vec<u16>> =
        ImageBuffer::from_pixel(8, 8, Rgba([100, 200, 300, u16::MAX]));
    assert_eq!(
        format_of(&DynamicImage::ImageRgba16(wide.clone()), "w.png"),
        TierFormat::Jpeg
    );
    wide.put_pixel(0, 0, Rgba([100, 200, 300, u16::MAX - 1]));
    assert_eq!(
        format_of(&DynamicImage::ImageRgba16(wide), "w.png"),
        TierFormat::Png
    );
}

#[test]
fn png_tiers_have_their_own_size_and_never_upscale() {
    let mut big = source(1200, 800).to_rgba8();
    big.put_pixel(0, 0, Rgba([0, 0, 0, 0]));
    let temp = TempDir::new("png-sizes");
    let store = CacheStore::new(temp.path());
    let entry = store
        .write_all(
            &DynamicImage::ImageRgba8(big),
            "gallery",
            "abcd1234",
            "b.png",
        )
        .expect("write the tiers");
    assert_eq!(longest(decoded_size(&entry.thumb256)), 256);
    assert_eq!(longest(decoded_size(&entry.preview1024)), 1024);
    assert_eq!(longest(decoded_size(&entry.full)), 1200);

    let mut small = source(40, 30).to_rgba8();
    small.put_pixel(0, 0, Rgba([0, 0, 0, 0]));
    let entry = store
        .write_all(
            &DynamicImage::ImageRgba8(small),
            "gallery",
            "small",
            "s.png",
        )
        .expect("write the tiers");
    for tier in Tier::ALL {
        assert_eq!(decoded_size(entry.path(tier)), (40, 30));
    }
}

#[test]
fn rewriting_opaque_as_alpha_removes_the_jpeg_files() {
    let temp = TempDir::new("to-alpha");
    let store = CacheStore::new(temp.path());
    let first = store
        .write_all(&source(30, 30), "gallery", "abcd1234", "art.png")
        .expect("first write");
    let mut alpha = source(30, 30).to_rgba8();
    alpha.put_pixel(1, 1, Rgba([0, 0, 0, 0]));
    let second = store
        .write_all(
            &DynamicImage::ImageRgba8(alpha),
            "gallery",
            "abcd1234",
            "art.png",
        )
        .expect("second write");
    assert_eq!(first.format, TierFormat::Jpeg);
    assert_eq!(second.format, TierFormat::Png);
    assert!(second.all_files_exist());
    for tier in Tier::ALL {
        assert!(!first.path(tier).exists(), "a stale JPEG is left behind");
    }
}

#[test]
fn rewriting_alpha_as_opaque_removes_the_png_files() {
    let temp = TempDir::new("to-opaque");
    let store = CacheStore::new(temp.path());
    let mut alpha = source(30, 30).to_rgba8();
    alpha.put_pixel(1, 1, Rgba([0, 0, 0, 0]));
    let first = store
        .write_all(
            &DynamicImage::ImageRgba8(alpha),
            "gallery",
            "abcd1234",
            "art.png",
        )
        .expect("first write");
    let second = store
        .write_all(&source(30, 30), "gallery", "abcd1234", "art.png")
        .expect("second write");
    assert_eq!(second.format, TierFormat::Jpeg);
    assert!(second.all_files_exist());
    for tier in Tier::ALL {
        assert!(!first.path(tier).exists(), "a stale PNG is left behind");
    }
}

#[test]
fn png_entries_have_the_marker_and_no_temp_file() {
    let temp = TempDir::new("png-marker");
    let store = CacheStore::new(temp.path());
    let entry = store
        .write_all(
            &super::transparent_source(10, 10),
            "gallery",
            "abcd1234",
            "a.png",
        )
        .expect("write the tiers");
    for tier in Tier::ALL {
        assert!(
            temp.path()
                .join(tier.as_str())
                .join(FORMAT_MARKER)
                .is_file()
        );
        let dir = entry.path(tier).parent().expect("parent").to_path_buf();
        for item in std::fs::read_dir(dir).expect("read the dir") {
            let name = item.expect("entry").file_name();
            assert!(!name.to_string_lossy().contains(".tmp"));
        }
    }
}

#[test]
fn freshness_holds_for_both_formats_and_fails_for_a_mixed_set() {
    let temp = TempDir::new("fresh");
    let store = CacheStore::new(temp.path());
    let stamp = SourceStamp { mtime: 1, size: 2 };
    let jpeg = store
        .write_all(&source(30, 30), "gallery", "j", "a.png")
        .expect("jpeg write");
    let png = store
        .write_all(&super::transparent_source(30, 30), "gallery", "p", "a.png")
        .expect("png write");
    assert!(jpeg.is_fresh(stamp, stamp));
    assert!(png.is_fresh(stamp, stamp));

    // Swap one PNG tier for a JPEG-named file: the set now mixes extensions.
    let jpeg_twin = png.sibling();
    std::fs::rename(&png.preview1024, &jpeg_twin.preview1024).expect("rename one tier");
    assert!(!png.is_fresh(stamp, stamp));
    assert!(!jpeg_twin.is_fresh(stamp, stamp));
}

#[test]
fn find_entry_probes_png_then_jpeg() {
    let temp = TempDir::new("find");
    let store = CacheStore::new(temp.path());
    assert!(
        store
            .find_entry("gallery", "p", "a.png")
            .expect("valid")
            .is_none()
    );
    let png = store
        .write_all(&super::transparent_source(10, 10), "gallery", "p", "a.png")
        .expect("png write");
    let jpeg = store
        .write_all(&source(10, 10), "gallery", "j", "a.png")
        .expect("jpeg write");
    assert_eq!(
        store.find_entry("gallery", "p", "a.png").expect("ok"),
        Some(png)
    );
    assert_eq!(
        store.find_entry("gallery", "j", "a.png").expect("ok"),
        Some(jpeg)
    );
    assert!(store.find_entry("gallery", "../x", "a.png").is_err());
}

/// Seed the full-tier mtime of one complete set, in seconds after the epoch.
fn seed_set(store: &CacheStore, format: TierFormat, secs: u64) {
    let entry = store
        .entry("gallery", "both", "a.png", format)
        .expect("valid names");
    for tier in Tier::ALL {
        let path = entry.path(tier);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("create the dir");
        let file = std::fs::File::create(path).expect("create the file");
        file.set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(secs))
            .expect("set the mtime");
    }
}

#[test]
fn find_entry_prefers_the_newer_set_and_a_tie_goes_to_png() {
    for (png_secs, jpeg_secs, expected) in [
        (2000, 1000, TierFormat::Png),
        (1000, 2000, TierFormat::Jpeg),
        (1500, 1500, TierFormat::Png),
    ] {
        let temp = TempDir::new("both-sets");
        let store = CacheStore::new(temp.path());
        seed_set(&store, TierFormat::Png, png_secs);
        seed_set(&store, TierFormat::Jpeg, jpeg_secs);
        let found = store
            .find_entry("gallery", "both", "a.png")
            .expect("valid")
            .expect("a complete set exists");
        assert_eq!(found.format, expected, "png {png_secs} jpeg {jpeg_secs}");
    }
}
