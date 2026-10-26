use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use image::{DynamicImage, Rgba, RgbaImage};
use tarragon_core::tier::Tier;

use crate::CacheError;

use super::{CacheStore, FORMAT_MARKER, SourceStamp, TierFormat, is_stale};

/// A temporary cache root that removes itself on drop.
struct TempDir {
    path: PathBuf,
}

impl TempDir {
    /// Create a unique temp folder with the given tag.
    fn new(tag: &str) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("tarragon-cache-{tag}-{}-{id}", std::process::id()));
        std::fs::create_dir_all(&path).expect("create the temp folder");
        Self { path }
    }

    /// Return the folder path.
    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// Build a deterministic RGBA source image.
fn source(width: u32, height: u32) -> DynamicImage {
    let mut rgba = RgbaImage::new(width, height);
    for (x, y, pixel) in rgba.enumerate_pixels_mut() {
        let red = u8::try_from((x * 7) % 256).unwrap_or(0);
        let green = u8::try_from((y * 5) % 256).unwrap_or(0);
        let blue = u8::try_from((x + y) % 256).unwrap_or(0);
        *pixel = Rgba([red, green, blue, 255]);
    }
    DynamicImage::ImageRgba8(rgba)
}

/// Build a source with every pixel fully transparent.
fn transparent_source(width: u32, height: u32) -> DynamicImage {
    let rgba = RgbaImage::from_pixel(width, height, Rgba([10, 20, 30, 0]));
    DynamicImage::ImageRgba8(rgba)
}

/// Decode a tier file and return its pixel size.
fn decoded_size(path: &Path) -> (u32, u32) {
    let image = image::open(path).expect("decode the tier file");
    (image.width(), image.height())
}

/// Return the longest edge of a size.
fn longest(size: (u32, u32)) -> u32 {
    size.0.max(size.1)
}

mod format;

#[test]
fn cache_paths_follow_the_tier_layout() {
    let temp = TempDir::new("layout");
    let store = CacheStore::new(temp.path());
    let entry = store
        .entry("gallery", "abcd1234", "art.jpg", TierFormat::Jpeg)
        .expect("valid names");

    assert_eq!(
        entry.thumb256,
        temp.path()
            .join("256")
            .join("gallery_abcd1234")
            .join("art.jpg.jpg")
    );
    assert_eq!(
        entry.preview1024,
        temp.path()
            .join("1024")
            .join("gallery_abcd1234")
            .join("art.jpg.jpg")
    );
    assert_eq!(
        entry.full,
        temp.path()
            .join("full")
            .join("gallery_abcd1234")
            .join("art.jpg.jpg")
    );
}

#[test]
fn an_opaque_png_source_is_stored_with_a_jpg_name_and_jpeg_bytes() {
    let temp = TempDir::new("png-source");
    let store = CacheStore::new(temp.path());
    let entry = store
        .write_all(&source(300, 200), "gallery", "abcd1234", "photo.png")
        .expect("write the tiers");

    for tier in Tier::ALL {
        let file = entry.path(tier);
        assert_eq!(
            file.file_name().and_then(|name| name.to_str()),
            Some("photo.png.jpg"),
            "tier {} did not get the .jpg payload name",
            tier.as_str()
        );
        let bytes = std::fs::read(file).expect("read the tier file");
        assert_eq!(
            bytes.get(..2),
            Some([0xFF, 0xD8].as_slice()),
            "tier {} is not a JPEG",
            tier.as_str()
        );
        let decoded = image::open(file).expect("a .png source must open as JPEG");
        assert!(decoded.width() > 0 && decoded.height() > 0);
    }
}

#[test]
fn same_stem_sources_get_distinct_cache_names() {
    let temp = TempDir::new("distinct");
    let store = CacheStore::new(temp.path());
    let jpg = store
        .entry("gallery", "abcd1234", "photo.jpg", TierFormat::Jpeg)
        .expect("valid names");
    let png = store
        .entry("gallery", "abcd1234", "photo.png", TierFormat::Jpeg)
        .expect("valid names");

    assert_eq!(
        jpg.thumb256.file_name().and_then(|name| name.to_str()),
        Some("photo.jpg.jpg")
    );
    assert_eq!(
        png.thumb256.file_name().and_then(|name| name.to_str()),
        Some("photo.png.jpg")
    );
    assert_ne!(jpg.thumb256, png.thumb256);
}

#[test]
fn each_tier_is_written_at_its_own_size() {
    let temp = TempDir::new("tiers");
    let store = CacheStore::new(temp.path());
    let entry = store
        .write_all(&source(1200, 800), "gallery", "abcd1234", "art.jpg")
        .expect("write the tiers");

    assert_eq!(longest(decoded_size(&entry.thumb256)), 256);
    assert_eq!(longest(decoded_size(&entry.preview1024)), 1024);
    assert_eq!(longest(decoded_size(&entry.full)), 1200);
}

#[test]
fn a_small_source_is_not_upscaled() {
    let temp = TempDir::new("small");
    let store = CacheStore::new(temp.path());
    let entry = store
        .write_all(&source(40, 30), "gallery", "abcd1234", "small.jpg")
        .expect("write the tiers");

    for tier in Tier::ALL {
        assert_eq!(
            decoded_size(entry.path(tier)),
            (40, 30),
            "tier {} upscaled a small source",
            tier.as_str()
        );
    }
}

#[test]
fn transparent_pixels_keep_their_alpha_in_png_tiers() {
    let temp = TempDir::new("keep-alpha");
    let store = CacheStore::new(temp.path());
    let entry = store
        .write_all(
            &transparent_source(40, 30),
            "gallery",
            "abcd1234",
            "alpha.jpg",
        )
        .expect("write the tiers");

    assert_eq!(entry.format, TierFormat::Png);
    for tier in Tier::ALL {
        let file = entry.path(tier);
        assert_eq!(
            file.file_name().and_then(|name| name.to_str()),
            Some("alpha.jpg.png")
        );
        let decoded = image::open(file).expect("decode the tier").to_rgba8();
        assert!(
            decoded.pixels().all(|pixel| pixel.0[3] == 0),
            "tier {} lost its alpha",
            tier.as_str()
        );
    }
}

#[test]
fn the_full_tier_respects_the_8192_px_cap() {
    let temp = TempDir::new("cap");
    let store = CacheStore::new(temp.path());
    let entry = store
        .write_all(&source(9000, 10), "gallery", "abcd1234", "wide.jpg")
        .expect("write the tiers");

    assert_eq!(longest(decoded_size(&entry.full)), 8192);
}

#[test]
fn write_leaves_no_temp_file_and_the_tier_file_is_present() {
    let temp = TempDir::new("atomic");
    let store = CacheStore::new(temp.path());
    let entry = store
        .write_all(&source(300, 200), "gallery", "abcd1234", "art.jpg")
        .expect("write the tiers");

    for tier in Tier::ALL {
        let file = entry.path(tier);
        assert!(file.is_file(), "tier file is missing: {}", file.display());
        let dir = file.parent().expect("tier file has a parent");
        for item in std::fs::read_dir(dir).expect("read the tier dir") {
            let name = item.expect("read a dir entry").file_name();
            let name = name.to_string_lossy();
            assert!(
                !name.contains(".tmp"),
                "a temp file was left behind: {name}"
            );
        }
    }
}

#[test]
fn every_tier_holds_the_format_marker() {
    let temp = TempDir::new("marker");
    let store = CacheStore::new(temp.path());
    store
        .write_all(&source(300, 200), "gallery", "abcd1234", "art.jpg")
        .expect("write the tiers");

    for tier in Tier::ALL {
        let marker = store.marker_path(tier);
        assert!(marker.is_file(), "marker is missing: {}", marker.display());
        assert_eq!(
            marker,
            temp.path().join(tier.as_str()).join(FORMAT_MARKER),
            "marker is not in the tier directory"
        );
    }
}

#[test]
fn staleness_detects_a_changed_mtime_or_size() {
    let stored = SourceStamp {
        mtime: 1000,
        size: 500,
    };
    assert!(!is_stale(stored, stored), "an equal stamp is fresh");
    assert!(
        is_stale(
            stored,
            SourceStamp {
                mtime: 1001,
                size: 500
            }
        ),
        "a changed mtime is stale"
    );
    assert!(
        is_stale(
            stored,
            SourceStamp {
                mtime: 1000,
                size: 501
            }
        ),
        "a changed size is stale"
    );
}

#[test]
fn a_fresh_cache_entry_is_a_hit() {
    let temp = TempDir::new("hit");
    let store = CacheStore::new(temp.path());
    let entry = store
        .write_all(&source(300, 200), "gallery", "abcd1234", "art.jpg")
        .expect("write the tiers");
    let stamp = SourceStamp {
        mtime: 1000,
        size: 5000,
    };

    assert!(entry.all_files_exist(), "all three tiers were written");
    assert!(entry.is_fresh(stamp, stamp), "an equal stamp is a hit");
}

#[test]
fn a_missing_tier_file_is_a_miss() {
    let temp = TempDir::new("miss");
    let store = CacheStore::new(temp.path());
    let entry = store
        .write_all(&source(300, 200), "gallery", "abcd1234", "art.jpg")
        .expect("write the tiers");
    let stamp = SourceStamp {
        mtime: 1000,
        size: 5000,
    };

    std::fs::remove_file(&entry.preview1024).expect("remove one tier");

    assert!(!entry.all_files_exist(), "one tier file is gone");
    assert!(
        !entry.is_fresh(stamp, stamp),
        "a missing tier file must be a miss"
    );
}

#[test]
fn a_name_with_a_separator_or_dot_dot_is_refused() {
    let temp = TempDir::new("names");
    let store = CacheStore::new(temp.path());
    for (folder, uuid, name) in [
        ("a/b", "abcd", "art.jpg"),
        ("gallery", "../x", "art.jpg"),
        ("gallery", "abcd", "sub/art.jpg"),
        ("gallery", "abcd", ".."),
        ("gallery", "abcd", "."),
    ] {
        assert!(
            matches!(
                store.entry(folder, uuid, name, TierFormat::Jpeg),
                Err(CacheError::InvalidName(_))
            ),
            "{folder} {uuid} {name} must be refused"
        );
        assert!(
            store.write_all(&source(4, 4), folder, uuid, name).is_err(),
            "write_all must refuse {folder} {uuid} {name}"
        );
    }
}

#[cfg(windows)]
#[test]
fn a_backslash_is_refused_on_windows() {
    let store = CacheStore::new(std::env::temp_dir());
    assert!(matches!(
        store.entry("gallery", "abcd", "..\\art.jpg", TierFormat::Jpeg),
        Err(CacheError::InvalidName(_))
    ));
}

#[cfg(unix)]
#[test]
fn a_backslash_is_a_valid_name_byte_on_unix() {
    let temp = TempDir::new("backslash");
    let store = CacheStore::new(temp.path());
    let entry = store
        .write_all(&source(4, 4), "gallery", "abcd", "a\\b.png")
        .expect("a backslash is an ordinary byte here");
    assert!(entry.all_files_exist());
}

#[test]
fn a_long_source_name_still_writes() {
    let temp = TempDir::new("long");
    let store = CacheStore::new(temp.path());
    // 251 bytes plus ".jpg" is the longest cache name NAME_MAX allows.
    let name = "n".repeat(251);
    let entry = store
        .write_all(&source(8, 8), "gallery", "abcd1234", &name)
        .expect("a long name must still write");
    assert!(entry.all_files_exist());
}
