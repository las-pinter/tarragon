//! Tier directory layout, cache keys, atomic writes, and format markers.
//!
//! The path is `<root>/<tier>/<folder>_<uuid>/<source_name>.<ext>`, with the
//! lowercase tiers `256`, `1024`, and `full`. The `full` tier uses the source
//! size up to the 8192 px cap.
//!
//! Image scaling and encoding live in the private `codec` module.
//!
//! A source with visible alpha is stored as PNG (`.png`). Any other source is
//! stored as JPEG q90 (`.jpg`). The format is chosen once per source, so the
//! three tiers share one extension. The cache file name is the source base
//! name plus that payload extension. The store never trusts the source
//! extension, which can lie about the payload, so an opaque `photo.png`
//! becomes `photo.png.jpg`.
//!
//! Every tier file is written through a temp file and an atomic rename. Each
//! tier directory holds one `format=1` marker.
//!
//! Staleness uses the source mtime in whole seconds and the source size. A
//! cache hit needs a matching source stamp and all three tier files, each
//! present and non-empty.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use image::DynamicImage;
use tarragon_core::tier::Tier;

use crate::CacheError;
use crate::codec::{encode_tier, fit_to_edge};

pub use crate::codec::{JPEG_QUALITY, TierFormat};

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
///
/// All three paths share the extension of `format`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CacheEntry {
    /// The format of the three tier files.
    pub format: TierFormat,
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

    /// Return the same entry with the tier files of the other format.
    ///
    /// Only the extension changes. The result tells where a rewrite in the
    /// other format would leave files behind.
    #[must_use]
    pub fn sibling(&self) -> Self {
        let other = self.format.other();
        let swap = |path: &Path| path.with_extension(other.extension());
        Self {
            format: other,
            thumb256: swap(&self.thumb256),
            preview1024: swap(&self.preview1024),
            full: swap(&self.full),
        }
    }

    /// Return true when all three tier files exist and none is empty.
    ///
    /// A power loss can leave a zero-length file, which is a miss.
    #[must_use]
    pub fn all_files_exist(&self) -> bool {
        Tier::ALL.iter().all(|tier| {
            std::fs::metadata(self.path(*tier)).is_ok_and(|m| m.is_file() && m.len() > 0)
        })
    }

    /// Return true when the entry is a cache hit.
    ///
    /// A hit needs a matching source stamp and all three tier files in the
    /// entry's own format. A missing or empty tier file, or a set that mixes
    /// extensions, is a miss.
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

    /// Build the three tier file paths for `format` without writing.
    ///
    /// `file_name` is the source base name. The store appends the payload
    /// extension of `format`, so the cache name never lies about the payload.
    /// An opaque `.png` source yields `photo.png.jpg`.
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
        format: TierFormat,
    ) -> Result<CacheEntry, CacheError> {
        check_component(folder)?;
        check_component(uuid)?;
        check_component(file_name)?;
        if file_name == "." || file_name == ".." {
            return Err(CacheError::InvalidName(file_name.to_owned()));
        }
        let cache_name = cache_name(file_name, format);
        Ok(CacheEntry {
            format,
            thumb256: self
                .tier_dir(Tier::Thumb256, folder, uuid)
                .join(&cache_name),
            preview1024: self
                .tier_dir(Tier::Preview1024, folder, uuid)
                .join(&cache_name),
            full: self.tier_dir(Tier::Full, folder, uuid).join(&cache_name),
        })
    }

    /// Find the stored entry for a source when no DB row gives the format.
    ///
    /// The probe tries PNG first, then JPEG. When only one set is complete, it
    /// wins. When both are complete (a crash can leave the old set after a
    /// format change), the set whose `full` tier file is newer wins, and a
    /// tie goes to PNG. It returns `None` when neither set is complete.
    ///
    /// # Errors
    ///
    /// Returns [`CacheError::InvalidName`] for a name that
    /// [`CacheStore::entry`] refuses.
    pub fn find_entry(
        &self,
        folder: &str,
        uuid: &str,
        file_name: &str,
    ) -> Result<Option<CacheEntry>, CacheError> {
        let mut best: Option<(CacheEntry, std::time::SystemTime)> = None;
        for format in TierFormat::PROBE_ORDER {
            let entry = self.entry(folder, uuid, file_name, format)?;
            if !entry.all_files_exist() {
                continue;
            }
            let stamp = std::fs::metadata(&entry.full)
                .and_then(|metadata| metadata.modified())
                .unwrap_or(std::time::UNIX_EPOCH);
            // PNG probes first, so a tie keeps it.
            if best.as_ref().is_none_or(|(_, kept)| stamp > *kept) {
                best = Some((entry, stamp));
            }
        }
        Ok(best.map(|(entry, _)| entry))
    }

    /// Write the three tiers from `source` and return their paths.
    ///
    /// `file_name` is the source base name. The format is PNG when `source`
    /// has an alpha channel and at least one pixel below full alpha, else
    /// JPEG q90. The choice is made once on the source, so all three tiers
    /// share one extension. The returned entry carries the chosen format, so
    /// the caller can store the real paths. The caller must store those
    /// paths and treat a missing or empty file as a cache miss.
    ///
    /// Each tier is scaled to its own longest edge and is never upscaled.
    /// The `full` tier is capped at [`FULL_EDGE_CAP`]. Every file is written
    /// through a temp file and an atomic rename. The `format=1` marker is
    /// ensured in each tier directory. When the source changed format since
    /// an earlier write, the same-name files of the other extension are
    /// removed after the new set is in place.
    ///
    /// # Errors
    ///
    /// Returns [`CacheError::InvalidName`] for a name that [`CacheStore::entry`]
    /// refuses. Returns [`CacheError::Io`] when a directory or file operation
    /// fails. Returns [`CacheError::Encode`] when an encoder fails.
    ///
    /// `folder` is the source folder name and `uuid` is that folder's stable
    /// id. Together they key the tier directory. A bad character in either
    /// gives [`CacheError::InvalidName`].
    pub fn write_all(
        &self,
        source: &DynamicImage,
        folder: &str,
        uuid: &str,
        file_name: &str,
    ) -> Result<CacheEntry, CacheError> {
        let format = TierFormat::for_source(source);
        let entry = self.entry(folder, uuid, file_name, format)?;
        for tier in Tier::ALL {
            let edge = tier.max_edge().unwrap_or(FULL_EDGE_CAP);
            let fitted = fit_to_edge(source, edge);
            self.ensure_marker(tier)?;
            write_atomic(&fitted, format, entry.path(tier))?;
        }
        let stale = entry.sibling();
        for tier in Tier::ALL {
            // Best effort: a leftover file of the other format is only waste,
            // and the new set is already complete.
            let _ = std::fs::remove_file(stale.path(tier));
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
/// The name is the source base name plus the payload extension of `format`.
/// The store never trusts the source extension, which can lie about the
/// payload, so an opaque `photo.png` becomes `photo.png.jpg`.
#[must_use]
fn cache_name(source_name: &str, format: TierFormat) -> String {
    format!("{source_name}.{}", format.extension())
}

/// Encode `image` as `format` and write it at `dest` through a temp file and
/// an atomic rename.
///
/// The temp file lives in the destination directory, so the rename stays on
/// one filesystem. A failed write removes the temp file.
fn write_atomic(image: &DynamicImage, format: TierFormat, dest: &Path) -> Result<(), CacheError> {
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

    // No fsync here. A power loss can leave an empty tier file, and an empty
    // file counts as a miss, so the entry renders again. The cache can be
    // rebuilt. A partial non-empty file is not caught here, so a reader must
    // treat a decode failure as a miss.
    let written = encode_tier(image, format).and_then(|bytes| {
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
mod tests;
