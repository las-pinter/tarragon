//! Tier directory layout, cache keys, atomic writes, and format markers.
//!
//! The path is `<root>/<tier>/<folder>_<uuid>/<source_name>.jpg`, with the
//! lowercase tiers `256`, `1024`, and `full`. The `full` tier uses the source
//! size up to the 8192 px cap.
//!
//! Image scaling and JPEG encoding live in the private `codec` module.
//!
//! The cache file name is the source base name plus the `.jpg` payload
//! extension. The store never trusts the source extension, which can lie about
//! the payload, so `photo.png` becomes `photo.png.jpg`.
//!
//! Every tier file is written through a temp file and an atomic rename. Each
//! tier directory holds one `format=1` marker.
//!
//! Staleness uses the source mtime in whole seconds and the source size. A
//! cache hit needs a matching source stamp and all three tier files.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use image::{DynamicImage, RgbImage};
use tarragon_core::tier::Tier;

use crate::CacheError;
use crate::codec::{encode_jpeg, fit_to_edge, flatten_onto_white};

pub use crate::codec::JPEG_QUALITY;

/// The 8192 px render cap for the `full` tier.
pub const FULL_EDGE_CAP: u32 = 8192;

/// The name prefix of an in-flight temp file. A crash can leave one behind.
pub(crate) const TEMP_PREFIX: &str = ".tmp-";

/// The marker file name that records the cache format version.
pub const FORMAT_MARKER: &str = "format=1";

/// The source change stamp: mtime in whole seconds and size in bytes.
///
/// The stamp is the stored value from the DB row. A difference in either field
/// makes the cache entry stale.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceStamp {
    /// Source last-modified time in whole epoch seconds.
    pub mtime: i64,
    /// Source file size in bytes.
    pub size: u64,
}

/// Return true when the stored source stamp differs from the current one.
///
/// Staleness uses the source mtime in whole seconds and the source size. A
/// difference in either field is stale.
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
    /// A hit needs a matching source stamp and all three tier files. A
    /// missing tier file is a miss.
    #[must_use]
    pub fn is_fresh(&self, stored: SourceStamp, source: SourceStamp) -> bool {
        !is_stale(stored, source) && self.all_files_exist()
    }
}

/// A cache store rooted at one cache directory.
///
/// The layout is `<root>/<tier>/<folder>_<uuid>/<file_name>`.
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
    /// payload extension, so the cache name never lies about the format. A
    /// `.png` source yields `photo.png.jpg`.
    ///
    /// # Errors
    ///
    /// Returns [`CacheError::InvalidName`] when `folder`, `uuid`, or
    /// `file_name` holds a path separator (`/`, and `\` on Windows) or a NUL
    /// byte, or when `file_name`
    /// is `.` or `..`. Such a name could leave the cache root.
    pub fn entry(
        &self,
        folder: &str,
        uuid: &str,
        file_name: &str,
    ) -> Result<CacheEntry, CacheError> {
        check_component(folder)?;
        check_component(uuid)?;
        check_component(file_name)?;
        if file_name == "." || file_name == ".." {
            return Err(CacheError::InvalidName(file_name.to_owned()));
        }
        let cache_name = cache_jpg_name(file_name);
        Ok(CacheEntry {
            thumb256: self
                .tier_dir(Tier::Thumb256, folder, uuid)
                .join(&cache_name),
            preview1024: self
                .tier_dir(Tier::Preview1024, folder, uuid)
                .join(&cache_name),
            full: self.tier_dir(Tier::Full, folder, uuid).join(&cache_name),
        })
    }

    /// Write the three tiers from `source` and return their paths.
    ///
    /// `file_name` is the source base name. The cache name appends the `.jpg`
    /// payload extension (see [`CacheStore::entry`]).
    ///
    /// Each tier is scaled to its own longest edge and is never upscaled.
    /// The `full` tier is capped at [`FULL_EDGE_CAP`]. Every file is written
    /// through a temp file and an atomic rename. The `format=1` marker is
    /// ensured in each tier directory.
    ///
    /// # Errors
    ///
    /// Returns [`CacheError::InvalidName`] for a name that [`CacheStore::entry`]
    /// refuses. Returns [`CacheError::Io`] when a directory or file operation
    /// fails. Returns [`CacheError::Encode`] when the JPEG encoder fails.
    pub fn write_all(
        &self,
        source: &DynamicImage,
        folder: &str,
        uuid: &str,
        file_name: &str,
    ) -> Result<CacheEntry, CacheError> {
        let entry = self.entry(folder, uuid, file_name)?;
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

/// Refuse a name part that could leave its folder.
///
/// A backslash separates paths only on Windows. On Linux it is a valid
/// file-name byte, so a source named `a\b.png` must still cache.
fn check_component(part: &str) -> Result<(), CacheError> {
    if part.contains(['/', '\0']) || (cfg!(windows) && part.contains('\\')) {
        return Err(CacheError::InvalidName(part.to_owned()));
    }
    Ok(())
}

/// Return the cache file name for one source base name.
///
/// The name is the source base name plus the `.jpg` payload extension. The
/// store never trusts the source extension, which can lie about the payload,
/// so `photo.png` becomes `photo.png.jpg`.
#[must_use]
fn cache_jpg_name(source_name: &str) -> String {
    format!("{source_name}.jpg")
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
    // A short fixed name keeps the temp inside NAME_MAX for any final name.
    let temp = dest.with_file_name(format!("{TEMP_PREFIX}{}-{seq}", std::process::id()));

    let written = encode_jpeg(image).and_then(|bytes| write_synced(&temp, &bytes));
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

/// Write `bytes` to `path` and flush them to disk.
///
/// Without the sync, a power loss after the rename can leave an empty file
/// that still counts as a cache hit.
fn write_synced(path: &Path, bytes: &[u8]) -> Result<(), CacheError> {
    let io_error = |source| CacheError::Io {
        path: path.to_path_buf(),
        source,
    };
    let mut file = std::fs::File::create(path).map_err(io_error)?;
    file.write_all(bytes).map_err(io_error)?;
    file.sync_all().map_err(io_error)
}

#[cfg(test)]
mod tests;
