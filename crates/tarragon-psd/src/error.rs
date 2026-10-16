//! Errors for the PSD and PSB parser.
//!
//! The parser never panics on bad input. Every failure is a [`PsdError`]. The
//! conversion to the shared render error sits at the composite boundary that
//! task 18 adds.

use std::path::{Path, PathBuf};

use tarragon_core::error::RenderError;

/// A PSD or PSB parse failed.
#[derive(Debug, thiserror::Error)]
pub enum PsdError {
    /// The file cannot be read.
    #[error("cannot read {path}: {source}")]
    Io {
        /// The file path.
        path: PathBuf,
        /// The cause.
        #[source]
        source: std::io::Error,
    },
    /// The file signature is not `8BPS`.
    #[error("not a PSD or PSB file: signature {found:?}")]
    BadSignature {
        /// The four signature bytes.
        found: [u8; 4],
    },
    /// The version is not 1 (PSD) nor 2 (PSB).
    #[error("unsupported PSD version: {0}")]
    UnsupportedVersion(u16),
    /// The file ends before a field is complete.
    #[error("truncated file at offset {offset}: want {wanted} bytes, {remaining} left")]
    Truncated {
        /// The offset where the field starts.
        offset: u64,
        /// The number of bytes the field needs.
        wanted: usize,
        /// The number of bytes left in the file.
        remaining: usize,
    },
    /// A layer record does not carry the `8BIM` signature.
    #[error("bad layer blend signature: {found:?}")]
    BadBlendSignature {
        /// The four signature bytes.
        found: [u8; 4],
    },
    /// A section length does not fit the address space.
    #[error("section length {length} is too large")]
    LengthTooLarge {
        /// The declared length in bytes.
        length: u64,
    },
}

impl PsdError {
    /// Convert to the shared render error and tag the file path.
    ///
    /// The composite path returns [`RenderError`], so the parser error
    /// crosses the crate boundary through this method (task 18).
    #[must_use]
    pub fn into_render_error(self, path: &Path) -> RenderError {
        match self {
            PsdError::Io { source, .. } => RenderError::Io {
                path: path.to_path_buf(),
                source,
            },
            other => RenderError::Decode {
                path: path.to_path_buf(),
                reason: other.to_string(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use tarragon_core::error::RenderError;

    use crate::error::PsdError;

    #[test]
    fn a_parse_error_becomes_a_decode_error_with_the_path() {
        let error = PsdError::UnsupportedVersion(9);
        let rendered = error.into_render_error(Path::new("deep.psb"));
        assert!(
            matches!(&rendered, RenderError::Decode { path, .. } if path == Path::new("deep.psb"))
        );
        assert!(rendered.to_string().contains("version"));
    }
}
