//! Error types shared across the tarragon crates.

use std::path::PathBuf;

/// An error in the shared domain rules.
///
/// The parse errors for [`ColorBucket`](crate::color::ColorBucket),
/// [`Tier`](crate::tier::Tier), and [`TagSource`](crate::entity::TagSource)
/// live here.
#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    /// A color bucket name is unknown.
    #[error("unknown color bucket: {0}")]
    UnknownColorBucket(String),
    /// A cache tier key is unknown.
    #[error("unknown cache tier: {0}")]
    UnknownTier(String),
    /// A sort mode name is unknown.
    #[error("unknown sort mode: {0}")]
    UnknownSortMode(String),
    /// A tag source name is unknown.
    #[error("unknown tag source: {0}")]
    UnknownTagSource(String),
}

/// A folder walk or a metadata read failed.
#[derive(Debug, thiserror::Error)]
pub enum ScanError {
    /// A folder cannot be read.
    #[error("cannot read folder {path}: {source}")]
    ReadFolder {
        /// The folder path.
        path: PathBuf,
        /// The cause.
        #[source]
        source: std::io::Error,
    },
    /// File metadata cannot be read.
    #[error("cannot read metadata for {path}: {source}")]
    Metadata {
        /// The file path.
        path: PathBuf,
        /// The cause.
        #[source]
        source: std::io::Error,
    },
    /// The scan was cancelled.
    #[error("scan cancelled")]
    Cancelled,
    /// A filesystem watch failed.
    #[error("cannot watch {path}: {reason}")]
    Watch {
        /// The folder path.
        path: PathBuf,
        /// The backend message.
        reason: String,
    },
}

/// A render failed.
#[derive(Debug, thiserror::Error)]
pub enum RenderError {
    /// The image format is not supported.
    #[error("unsupported image format: {0}")]
    UnsupportedFormat(String),
    /// A file read failed.
    #[error("cannot read {path}: {source}")]
    Io {
        /// The file path.
        path: PathBuf,
        /// The cause.
        #[source]
        source: std::io::Error,
    },
    /// A decode failed.
    #[error("cannot decode {path}: {reason}")]
    Decode {
        /// The file path.
        path: PathBuf,
        /// The reason text.
        reason: String,
    },
    /// The image edge is larger than the cap.
    #[error("image edge {edge} px exceeds the {cap} px cap")]
    TooLarge {
        /// The longest image edge in pixels.
        edge: u32,
        /// The allowed cap in pixels.
        cap: u32,
    },
    /// The render was cancelled.
    #[error("render cancelled")]
    Cancelled,
}

/// The top-level application error.
///
/// It wraps the shared error types. A caller that only needs one layer still
/// gets the source error through [`std::error::Error::source`].
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    /// A shared domain error.
    #[error(transparent)]
    Core(#[from] CoreError),
    /// A scan error.
    #[error(transparent)]
    Scan(#[from] ScanError),
    /// A render error.
    #[error(transparent)]
    Render(#[from] RenderError),
    /// A plain message with no source error.
    #[error("{0}")]
    Message(String),
}

#[cfg(test)]
mod tests {
    use crate::error::{AppError, CoreError, RenderError, ScanError};

    #[test]
    fn app_error_wraps_a_core_error() {
        let error = AppError::from(CoreError::UnknownTier("512".to_owned()));
        assert_eq!(error.to_string(), "unknown cache tier: 512");
    }

    #[test]
    fn app_error_wraps_a_scan_error() {
        let error = AppError::from(ScanError::Cancelled);
        assert_eq!(error.to_string(), "scan cancelled");
    }

    #[test]
    fn app_error_wraps_a_render_error() {
        let error = AppError::from(RenderError::TooLarge {
            edge: 9000,
            cap: 8192,
        });
        assert!(error.to_string().contains("9000"));
    }

    #[test]
    fn core_error_reports_the_bad_name() {
        let error = CoreError::UnknownColorBucket("chartreuse".to_owned());
        assert_eq!(error.to_string(), "unknown color bucket: chartreuse");
    }
}
