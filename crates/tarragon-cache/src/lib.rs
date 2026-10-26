//! Cache store for tarragon: tier directories, cache keys, atomic writes, and
//! format markers.
//!
//! This crate performs I/O. It does not depend on the GUI or the database. The
//! three tiers are written as PNG when the source has visible alpha, else as
//! JPEG q90. Every tier file is written through a
//! temp file and an atomic rename. The size cap and the LRU eviction live in
//! [`eviction`]. The purge and the full-tier clear live in [`purge`].

use std::path::PathBuf;

mod codec;
pub mod eviction;
pub mod purge;
pub mod store;
pub mod temp;

pub use codec::thumb_image;

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
    /// A cache entry name would leave its folder.
    #[error("cache name {0:?} holds a path separator or a dot component")]
    InvalidName(String),
    /// The cache root is unsafe to purge.
    #[error("refusing to purge the unsafe cache root {0}")]
    UnsafeRoot(PathBuf),
    /// An image encoder refused the image.
    #[error("cannot encode image: {0}")]
    Encode(#[from] image::ImageError),
}
