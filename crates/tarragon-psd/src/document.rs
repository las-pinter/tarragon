//! The owning PSD document: the parsed file plus its byte source.
//!
//! The layer compositor opens one [`PsdDocument`], reads the layer
//! tree, and then reads one tile at a time. The document keeps the open file,
//! and a region read fetches only the rows it needs from it.

use std::path::Path;
use std::sync::Arc;

use crate::decode::{ImageRegion, Viewport};
use crate::error::PsdError;
use crate::limits::ParseOptions;
use crate::parse::{PsdFile, parse, parse_with};
use crate::source::Source;

/// A parsed PSD or PSB file that keeps its source open.
///
/// A file is read by range and never whole, so memory stays bounded for a
/// large PSB. The source is shared through [`Arc`], so a clone is cheap (one
/// `Arc` bump); the parsed layer tree is copied. Reads are positional, so many
/// threads can read one document at once.
#[derive(Debug, Clone)]
pub struct PsdDocument {
    /// The parsed structure.
    file: PsdFile,
    /// The open file, or the bytes of a document built from memory.
    source: Arc<Source>,
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
        let source = Source::open(path)?;
        let file = parse(&source, options)?;
        Ok(Self {
            file,
            source: Arc::new(source),
        })
    }

    /// Open a file and keep its layers when only the merged image is bad.
    ///
    /// A layered file whose merged image section is truncated or uses a
    /// compression the parser does not decode still opens. [`PsdFile::merged_error`]
    /// then holds the reason and the merged description is empty, so the caller
    /// can composite the layers. Use [`PsdDocument::open_with_options`] when a
    /// bad merged image must fail the open.
    ///
    /// # Errors
    ///
    /// Returns [`PsdError::Io`] when the file cannot be read. Returns a parse
    /// error when the bytes are malformed or truncated before the merged
    /// image, when the merged image is bad and the file has no layers, or
    /// when the deadline passed or the work was cancelled.
    pub fn open_lenient(path: &Path, options: &ParseOptions) -> Result<Self, PsdError> {
        let source = Source::open(path)?;
        let file = parse_with(&source, options, true)?;
        Ok(Self {
            file,
            source: Arc::new(source),
        })
    }

    /// Parse a document from owned bytes.
    ///
    /// # Errors
    ///
    /// Returns a parse error when the bytes are malformed or truncated.
    pub fn from_bytes(data: Vec<u8>, options: &ParseOptions) -> Result<Self, PsdError> {
        let source = Source::memory(data);
        let file = parse(&source, options)?;
        Ok(Self {
            file,
            source: Arc::new(source),
        })
    }

    /// The parsed file structure.
    #[must_use]
    pub fn file(&self) -> &PsdFile {
        &self.file
    }

    /// The byte source, for tests that count reads.
    #[cfg(test)]
    pub(crate) fn source(&self) -> &Source {
        &self.source
    }

    /// The message of the error that stopped the layer info scan of the tagged
    /// blocks, or `None` when the scan found nothing wrong.
    ///
    /// A bad tagged block does not fail the parse: the file keeps its merged
    /// image and has no layers from that block. This tells a caller why.
    #[must_use]
    pub fn layer_info_error(&self) -> Option<&str> {
        self.file.layer_info_error.as_deref()
    }

    /// Why the document pattern table was skipped, or `None` when it was read
    /// or absent. A skipped table leaves pattern fill layers unsupported.
    #[must_use]
    pub fn pattern_note(&self) -> Option<&str> {
        self.file.pattern_note.as_deref()
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
        self.file
            .read_merged_region_from(&self.source, viewport, options)
    }

    /// Decode the merged image over a viewport, keeping every `step`th pixel.
    ///
    /// # Errors
    ///
    /// Returns a typed error when the viewport is invalid, the output exceeds
    /// the cap, the color mode is unsupported, the deadline passed, or the
    /// bytes are truncated.
    pub fn read_merged_strided(
        &self,
        viewport: Viewport,
        step: u32,
        options: &ParseOptions,
    ) -> Result<ImageRegion, PsdError> {
        self.file
            .read_merged_strided_from(&self.source, viewport, step, options)
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
            .read_layer_region_from(&self.source, index, viewport, options)
    }
}

/// Compile-time guard: a caller can share a document and move a region
/// between threads.
const _: () = {
    const fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<PsdDocument>();
    assert_send_sync::<PsdFile>();
    assert_send_sync::<crate::composite::CompositeRegion>();
};
