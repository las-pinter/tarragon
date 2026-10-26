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
    /// The data directory cannot be found: no `data` folder sits beside the
    /// binary and the platform gives none.
    #[error(
        "no data directory: add a data folder beside the binary or set the home directory (APPDATA on Windows)"
    )]
    NoDataDir,
}

/// A folder walk or a filesystem watch failed.
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
    /// A renderer panicked. The render guard caught the panic.
    #[error("the renderer crashed on {path}: {message}")]
    Panicked {
        /// The file path.
        path: PathBuf,
        /// The panic message.
        message: String,
    },
    /// The render took longer than the time limit.
    #[error("rendering {path} took too long")]
    Timeout {
        /// The file path.
        path: PathBuf,
    },
    /// The caller stopped the render before it finished.
    #[error("rendering {path} was cancelled")]
    Cancelled {
        /// The file path.
        path: PathBuf,
    },
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
        let error = AppError::from(ScanError::Watch {
            path: std::path::PathBuf::from("/x"),
            reason: "gone".to_owned(),
        });
        assert_eq!(error.to_string(), "cannot watch /x: gone");
    }

    #[test]
    fn the_new_render_errors_say_what_happened() {
        let path = std::path::PathBuf::from("/a.psd");
        let panicked = RenderError::Panicked {
            path: path.clone(),
            message: "boom".to_owned(),
        };
        assert_eq!(panicked.to_string(), "the renderer crashed on /a.psd: boom");
        let timeout = RenderError::Timeout { path: path.clone() };
        assert_eq!(timeout.to_string(), "rendering /a.psd took too long");
        let cancelled = RenderError::Cancelled { path };
        assert_eq!(cancelled.to_string(), "rendering /a.psd was cancelled");
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
