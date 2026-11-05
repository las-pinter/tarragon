//! Cache upkeep that needs both the cache files and the database.
//!
//! The cache crate works in bytes and cannot reach the database, so the size
//! cap is applied here.

use std::collections::HashMap;
use std::path::Path;

use tarragon_cache::CacheError;
use tarragon_cache::store::{CacheEntry, CacheStore, TierFormat};
use tarragon_db::{Database, DbError};

/// The bytes in one GiB (2^30). The cache cap setting counts in GiB.
const BYTES_PER_GIB: u64 = 1 << 30;

/// What one eviction pass did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Evicted {
    /// The `thumbnails` rows deleted.
    pub rows: usize,
    /// The total cache size in bytes after the pass.
    pub remaining_bytes: u64,
}

/// Why an eviction pass failed.
#[derive(Debug, thiserror::Error)]
pub enum MaintenanceError {
    /// A database call failed.
    #[error(transparent)]
    Db(#[from] DbError),
    /// A cache call failed.
    #[error(transparent)]
    Cache(#[from] CacheError),
}

/// Convert the cap setting from GiB (2^30 bytes) to bytes.
///
/// A negative value gives zero, which turns the cap off.
fn cap_to_bytes(cap_gb: i64) -> u64 {
    u64::try_from(cap_gb)
        .unwrap_or(0)
        .saturating_mul(BYTES_PER_GIB)
}

/// Evict the least recently used sources until the cache fits `cap_gb` GiB.
///
/// A cap of zero or less turns the cap off. The tier files and the
/// `thumbnails` row of each evicted source are deleted.
///
/// # Errors
///
/// Returns [`MaintenanceError`] when the row list or the cache pass fails.
pub fn evict(db: &Database, cache: &CacheStore, cap_gb: i64) -> Result<Evicted, MaintenanceError> {
    evict_bytes(db, cache, cap_to_bytes(cap_gb))
}

/// Evict the least recently used sources until the cache fits `cap_bytes`.
///
/// A row without all three cache paths owns no files, so it is skipped and
/// never deleted. A failed row delete is logged and not counted, because the
/// files are gone and the render path rebuilds them for a leftover row.
///
/// # Errors
///
/// Returns [`MaintenanceError`] when the row list or the cache pass fails.
fn evict_bytes(
    db: &Database,
    cache: &CacheStore,
    cap_bytes: u64,
) -> Result<Evicted, MaintenanceError> {
    let rows = db.thumbnails_by_last_accessed()?;
    let mut order = Vec::new();
    let mut row_of: HashMap<&Path, &str> = HashMap::new();
    for row in &rows {
        let (Some(small), Some(medium), Some(full)) = (
            &row.thumbnail_cache_path,
            &row.preview_cache_path,
            &row.full_cache_path,
        ) else {
            continue;
        };
        let entry = CacheEntry {
            format: if Path::new(small)
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("png"))
            {
                TierFormat::Png
            } else {
                TierFormat::Jpeg
            },
            thumb320: small.into(),
            preview1024: medium.into(),
            full: full.into(),
        };
        row_of.insert(Path::new(small), row.path.as_str());
        order.push(entry);
    }
    let outcome = cache.evict_to_cap(cap_bytes, &order)?;
    let mut deleted = 0;
    for entry in &outcome.evicted {
        let Some(path) = row_of.get(entry.thumb320.as_path()) else {
            continue;
        };
        match db.delete_thumbnail(path) {
            Ok(count) => deleted += count,
            Err(error) => tracing::warn!("cannot delete the row for {path}: {error}"),
        }
    }
    if !outcome.evicted.is_empty() {
        tracing::info!(
            "evicted {} cached sources, {deleted} rows, {} bytes remain",
            outcome.evicted.len(),
            outcome.remaining_bytes
        );
    }
    Ok(Evicted {
        rows: deleted,
        remaining_bytes: outcome.remaining_bytes,
    })
}

#[cfg(test)]
mod tests;
