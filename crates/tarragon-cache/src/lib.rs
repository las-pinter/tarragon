//! Cache store for tarragon: tier directories, cache keys, atomic writes, and
//! format markers.
//!
//! This crate performs I/O. It does not depend on the GUI or the database. The
//! three Q23 tiers are written as JPEG q90 (Q28 revised). Every tier file is
//! written through a temp file and an atomic rename (Q15). The size cap and
//! the LRU eviction (Q25, Q33) live in [`eviction`]. The purge and the
//! full-tier clear (Q25) live in [`purge`].

use std::path::PathBuf;

pub mod eviction;
pub mod purge;
pub mod store;

/// A cache store error.
#[derive(Debug, thiserror::Error)]
pub enum CacheError {
    /// A cache directory or file operation failed.
    #[error("cache i/o failed at {path}: {source}")]
    Io {
        /// The path that failed.
        path: PathBuf,
        /// The cause.
        #[source]
        source: std::io::Error,
    },
    /// The cache root is unsafe to purge.
    #[error("refusing to purge the unsafe cache root {0}")]
    UnsafeRoot(PathBuf),
    /// The JPEG encoder refused the image.
    #[error("cannot encode JPEG: {0}")]
    Encode(#[from] image::ImageError),
}
