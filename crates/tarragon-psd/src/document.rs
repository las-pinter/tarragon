//! The owning PSD document: the parsed file plus its bytes.
//!
//! The layer compositor (task 19) opens one [`PsdDocument`], reads the layer
//! tree, and then reads one tile at a time. The document owns the file bytes so
//! a region read does not re-read the file.

use std::path::Path;
use std::sync::Arc;

use crate::decode::{ImageRegion, Viewport};
use crate::error::PsdError;
use crate::limits::ParseOptions;
use crate::parse::{PsdFile, read_bytes_with_options};

/// A parsed PSD or PSB file that owns its bytes.
///
/// The file bytes are read into memory once and shared through [`Arc`], so
/// cloning the file bytes is cheap (one `Arc` bump); the parsed layer tree is
/// copied.
#[derive(Debug, Clone)]
pub struct PsdDocument {
    /// The parsed structure.
    file: PsdFile,
    /// The raw file bytes, read into memory once.
    data: Arc<[u8]>,
}

impl PsdDocument {
    /// Open and parse a file with the default 120 s deadline.
    ///
    /// # Errors
    ///
    /// Returns [`PsdError::Io`] when the file cannot be read. Returns a parse
    /// error when the bytes are malformed or truncated.
    pub fn open(path: &Path) -> Result<Self, PsdError> {
        Self::open_with_options(path, &ParseOptions::default())
    }

    /// Open and parse a file with explicit options.
    ///
    /// # Errors
    ///
    /// Returns [`PsdError::Io`] when the file cannot be read. Returns a parse
    /// error when the bytes are malformed or truncated.
    pub fn open_with_options(path: &Path, options: &ParseOptions) -> Result<Self, PsdError> {
        let data = std::fs::read(path).map_err(|source| PsdError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        Self::from_bytes(data, options)
    }

    /// Parse a document from owned bytes.
    ///
    /// # Errors
    ///
    /// Returns a parse error when the bytes are malformed or truncated.
    pub fn from_bytes(data: Vec<u8>, options: &ParseOptions) -> Result<Self, PsdError> {
        let data: Arc<[u8]> = Arc::from(data);
        let file = read_bytes_with_options(&data, options)?;
        Ok(Self { file, data })
    }

    /// The parsed file structure.
    #[must_use]
    pub fn file(&self) -> &PsdFile {
        &self.file
    }

    /// The raw file bytes.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.data
    }

    /// Decode the merged image over a viewport.
    ///
    /// # Errors
    ///
    /// Returns a typed error when the viewport is invalid, exceeds the cap,
    /// the color mode is unsupported, the deadline passed, or the bytes are
    /// truncated.
    pub fn read_merged_region(
        &self,
        viewport: Viewport,
        options: &ParseOptions,
    ) -> Result<ImageRegion, PsdError> {
        self.file.read_merged_region(&self.data, viewport, options)
    }

    /// Decode one layer's channels over a viewport.
    ///
    /// # Errors
    ///
    /// Returns a typed error when the layer index is out of range, the
    /// viewport is invalid or exceeds the cap, the color mode is unsupported,
    /// the deadline passed, or the bytes are truncated.
    pub fn read_layer_region(
        &self,
        index: usize,
        viewport: Viewport,
        options: &ParseOptions,
    ) -> Result<ImageRegion, PsdError> {
        self.file
            .read_layer_region(&self.data, index, viewport, options)
    }
}
