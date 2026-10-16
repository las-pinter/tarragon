//! Tier directory layout, cache keys, atomic writes, and format markers.
//!
//! The path is `<root>/<tier>/<folder>_<uuid>/<source_name>.jpg`, with the
//! lowercase tiers `256`, `1024`, and `full` (Q15, Q23). Each tier is scaled
//! to its own longest edge and is never upscaled. The `full` tier uses the
//! source size up to the 8192 px cap (Q9, Q23).
//!
//! The cache file name is the source base name plus the `.jpg` payload
//! extension. The store never trusts the source extension, which can lie about
//! the payload (Q28 revised). Python appends the cache extension to the whole
//! source basename, so `photo.png` becomes `photo.png.jpg`.
//!
//! Every tier file is written as JPEG q90 through a temp file and an atomic
//! rename (Q15, Q28 revised). Alpha flattens onto white `(255, 255, 255)`.
//! Each tier directory holds one `format=1` marker (Q15).
//!
//! Staleness uses the source mtime in whole seconds and the source size (Q15,
//! Q25). A cache hit needs a matching source stamp and all three tier files.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use image::codecs::jpeg::JpegEncoder;
use image::imageops::FilterType;
use image::{DynamicImage, Rgb, RgbImage};
use tarragon_core::tier::Tier;

use crate::CacheError;

/// The JPEG quality from Q28 (revised). Fixed.
pub const JPEG_QUALITY: u8 = 90;

/// The 8192 px render cap for the `full` tier (Q9, Q23).
pub const FULL_EDGE_CAP: u32 = 8192;

/// The marker file name that records the cache format version (Q15).
pub const FORMAT_MARKER: &str = "format=1";

/// The source change stamp: mtime in whole seconds and size in bytes.
///
/// The stamp is the stored value from the DB row (Q25). A difference in
/// either field makes the cache entry stale.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceStamp {
    /// Source last-modified time in whole epoch seconds.
    pub mtime: i64,
    /// Source file size in bytes.
    pub size: u64,
}

/// Return true when the stored source stamp differs from the current one.
///
/// Staleness uses the source mtime in whole seconds and the source size
/// (Q15, Q25). A difference in either field is stale.
#[must_use]
pub fn is_stale(stored: SourceStamp, source: SourceStamp) -> bool {
    stored != source
}

/// The three tier file paths for one cached source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CacheEntry {
    /// The `256` tier file.
    pub thumb256: PathBuf,
    /// The `1024` tier file.
    pub preview1024: PathBuf,
    /// The `full` tier file.
    pub full: PathBuf,
}

impl CacheEntry {
    /// Return the file path for `tier`.
    #[must_use]
    pub fn path(&self, tier: Tier) -> &Path {
        match tier {
            Tier::Thumb256 => &self.thumb256,
            Tier::Preview1024 => &self.preview1024,
            Tier::Full => &self.full,
        }
    }

    /// Return true when all three tier files exist on disk.
    #[must_use]
    pub fn all_files_exist(&self) -> bool {
        Tier::ALL.iter().all(|tier| self.path(*tier).is_file())
    }

    /// Return true when the entry is a cache hit.
    ///
    /// A hit needs a matching source stamp and all three tier files (Q15,
    /// Q25). A missing tier file is a miss.
    #[must_use]
    pub fn is_fresh(&self, stored: SourceStamp, source: SourceStamp) -> bool {
        !is_stale(stored, source) && self.all_files_exist()
    }
}

/// A cache store rooted at one cache directory.
///
/// The layout is `<root>/<tier>/<folder>_<uuid>/<file_name>` (Q15, Q23).
#[derive(Debug, Clone)]
pub struct CacheStore {
    root: PathBuf,
}

impl CacheStore {
    /// Create a store for `root`. The directories are created on the first
    /// write.
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// Return the cache root.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Return the directory for one tier and one source folder.
    #[must_use]
    pub fn tier_dir(&self, tier: Tier, folder: &str, uuid: &str) -> PathBuf {
        self.tier_root(tier).join(format!("{folder}_{uuid}"))
    }

    /// Return the `format=1` marker path for one tier.
    #[must_use]
    pub fn marker_path(&self, tier: Tier) -> PathBuf {
        self.tier_root(tier).join(FORMAT_MARKER)
    }

    /// Build the three tier file paths without writing.
    ///
    /// `file_name` is the source base name. The store appends the `.jpg`
    /// payload extension, so the cache name never lies about the format
    /// (Q28 revised). A `.png` source yields `photo.png.jpg`.
    #[must_use]
    pub fn entry(&self, folder: &str, uuid: &str, file_name: &str) -> CacheEntry {
        let cache_name = cache_jpg_name(file_name);
        CacheEntry {
            thumb256: self
                .tier_dir(Tier::Thumb256, folder, uuid)
                .join(&cache_name),
            preview1024: self
                .tier_dir(Tier::Preview1024, folder, uuid)
                .join(&cache_name),
            full: self.tier_dir(Tier::Full, folder, uuid).join(&cache_name),
        }
    }

    /// Write the three tiers from `source` and return their paths.
    ///
    /// `file_name` is the source base name. The cache name appends the `.jpg`
    /// payload extension (see [`CacheStore::entry`]).
    ///
    /// Each tier is scaled to its own Q23 longest edge and is never upscaled.
    /// The `full` tier is capped at [`FULL_EDGE_CAP`]. Every file is written
    /// through a temp file and an atomic rename. The `format=1` marker is
    /// ensured in each tier directory.
    ///
    /// # Errors
    ///
    /// Returns [`CacheError::Io`] when a directory or file operation fails.
    /// Returns [`CacheError::Encode`] when the JPEG encoder fails.
    pub fn write_all(
        &self,
        source: &DynamicImage,
        folder: &str,
        uuid: &str,
        file_name: &str,
    ) -> Result<CacheEntry, CacheError> {
        let entry = self.entry(folder, uuid, file_name);
        for tier in Tier::ALL {
            let edge = tier.max_edge().unwrap_or(FULL_EDGE_CAP);
            let fitted = fit_to_edge(source, edge);
            let rgb = flatten_onto_white(&fitted);
            self.ensure_marker(tier)?;
            write_jpeg_atomic(&rgb, entry.path(tier))?;
        }
        Ok(entry)
    }

    /// Return `<root>/<tier>`.
    fn tier_root(&self, tier: Tier) -> PathBuf {
        self.root.join(tier.as_str())
    }

    /// Create `<root>/<tier>/format=1` when it is absent.
    fn ensure_marker(&self, tier: Tier) -> Result<(), CacheError> {
        let dir = self.tier_root(tier);
        std::fs::create_dir_all(&dir).map_err(|source| CacheError::Io {
            path: dir.clone(),
            source,
        })?;
        let marker = dir.join(FORMAT_MARKER);
        if marker.is_file() {
            return Ok(());
        }
        std::fs::write(&marker, b"1").map_err(|source| CacheError::Io {
            path: marker,
            source,
        })
    }
}

/// Return the cache file name for one source base name.
///
/// The name is the source base name plus the `.jpg` payload extension. The
/// store never trusts the source extension, which can lie about the payload.
/// Python keeps the whole source basename and appends the cache extension, so
/// `photo.png` becomes `photo.png.jpg` (Q28 revised).
#[must_use]
fn cache_jpg_name(source_name: &str) -> String {
    format!("{source_name}.jpg")
}

/// Scale `source` to fit within `edge` on the longest side.
///
/// A source that already fits is cloned unchanged, so the cache never
/// upscales (Q23).
fn fit_to_edge(source: &DynamicImage, edge: u32) -> DynamicImage {
    if source.width() <= edge && source.height() <= edge {
        source.clone()
    } else {
        source.resize(edge, edge, FilterType::Lanczos3)
    }
}

/// Flatten alpha onto white, as Q28 (revised) fixes.
fn flatten_onto_white(source: &DynamicImage) -> RgbImage {
    let rgba = source.to_rgba8();
    let mut out = RgbImage::new(rgba.width(), rgba.height());
    let blend = |channel: u8, alpha: u32| -> u8 {
        let value = (u32::from(channel) * alpha + 255 * (255 - alpha) + 127) / 255;
        u8::try_from(value).unwrap_or(u8::MAX)
    };
    for (x, y, pixel) in rgba.enumerate_pixels() {
        let alpha = u32::from(pixel[3]);
        out.put_pixel(
            x,
            y,
            Rgb([
                blend(pixel[0], alpha),
                blend(pixel[1], alpha),
                blend(pixel[2], alpha),
            ]),
        );
    }
    out
}

/// Encode an RGB image as JPEG at the Q28 quality.
///
/// # Errors
///
/// Returns [`CacheError::Encode`] when the encoder fails.
fn encode_jpeg(image: &RgbImage) -> Result<Vec<u8>, CacheError> {
    let mut bytes = Vec::new();
    let mut encoder = JpegEncoder::new_with_quality(&mut bytes, JPEG_QUALITY);
    encoder.encode_image(image).map_err(CacheError::Encode)?;
    Ok(bytes)
}

/// Write `image` at `dest` through a temp file and an atomic rename.
///
/// The temp file lives in the destination directory, so the rename stays on
/// one filesystem. A failed write removes the temp file.
fn write_jpeg_atomic(image: &RgbImage, dest: &Path) -> Result<(), CacheError> {
    static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|source| CacheError::Io {
            path: parent.to_path_buf(),
            source,
        })?;
    }

    let seq = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
    let name = dest
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("cache");
    let temp = dest.with_file_name(format!(".{name}.tmp-{}-{seq}", std::process::id()));

    let written = encode_jpeg(image).and_then(|bytes| {
        std::fs::write(&temp, bytes).map_err(|source| CacheError::Io {
            path: temp.clone(),
            source,
        })
    });
    if let Err(error) = written {
        let _ = std::fs::remove_file(&temp);
        return Err(error);
    }

    if let Err(source) = std::fs::rename(&temp, dest) {
        let _ = std::fs::remove_file(&temp);
        return Err(CacheError::Io {
            path: dest.to_path_buf(),
            source,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};

    use image::{DynamicImage, Rgba, RgbaImage};
    use tarragon_core::tier::Tier;

    use super::{CacheStore, FORMAT_MARKER, SourceStamp, is_stale};

    /// A temporary cache root that removes itself on drop.
    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        /// Create a unique temp folder with the given tag.
        fn new(tag: &str) -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let id = NEXT.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir()
                .join(format!("tarragon-cache-{tag}-{}-{id}", std::process::id()));
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

    #[test]
    fn cache_paths_follow_the_tier_layout() {
        let temp = TempDir::new("layout");
        let store = CacheStore::new(temp.path());
        let entry = store.entry("gallery", "abcd1234", "art.jpg");

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
    fn a_png_source_is_stored_with_a_jpg_name_and_jpeg_bytes() {
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
        let jpg = store.entry("gallery", "abcd1234", "photo.jpg");
        let png = store.entry("gallery", "abcd1234", "photo.png");

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
    fn transparent_pixels_flatten_onto_white() {
        let temp = TempDir::new("flatten");
        let store = CacheStore::new(temp.path());
        let entry = store
            .write_all(
                &transparent_source(40, 30),
                "gallery",
                "abcd1234",
                "alpha.jpg",
            )
            .expect("write the tiers");

        let decoded = image::open(&entry.thumb256).expect("decode the 256 tier");
        for (x, y, pixel) in decoded.to_rgb8().enumerate_pixels() {
            for (channel, value) in pixel.0.iter().enumerate() {
                assert!(
                    *value >= 250,
                    "pixel ({x}, {y}) channel {channel} is not near white: {value}"
                );
            }
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
    fn jpeg_q90_output_decodes() {
        let temp = TempDir::new("jpeg");
        let store = CacheStore::new(temp.path());
        let entry = store
            .write_all(&source(300, 200), "gallery", "abcd1234", "art.jpg")
            .expect("write the tiers");

        for tier in Tier::ALL {
            let bytes = std::fs::read(entry.path(tier)).expect("read the tier file");
            assert_eq!(
                bytes.get(..2),
                Some([0xFF, 0xD8].as_slice()),
                "tier {} is not a JPEG",
                tier.as_str()
            );
            let decoded = image::open(entry.path(tier)).expect("decode the tier file");
            assert!(decoded.width() > 0 && decoded.height() > 0);
        }
    }
}
