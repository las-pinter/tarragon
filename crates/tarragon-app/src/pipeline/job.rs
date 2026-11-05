//! The job and result types that cross thread borders.

use std::collections::HashSet;
use std::path::PathBuf;

use image::RgbaImage;
use tarragon_cache::store::SourceStamp;
use tarragon_core::error::RenderError;
use tarragon_db::Database;
use tarragon_render::color::ColorTagConfig;

use crate::colortags;

/// Why a file has no thumbnail.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    /// The extension is not a supported format.
    UnsupportedFormat,
    /// The file could not be read.
    Io,
    /// The file could not be decoded.
    Decode,
    /// The image is larger than the cap.
    TooLarge,
    /// A renderer panicked.
    Panicked,
    /// The render ran too long.
    Timeout,
    /// The render was stopped.
    Cancelled,
    /// The file name cannot be a cache key.
    InvalidName,
}

impl ErrorKind {
    /// True when the same file will fail the same way next time. Only these
    /// failures are remembered; a read error or a timeout may pass later.
    #[must_use]
    pub fn is_permanent(self) -> bool {
        matches!(
            self,
            Self::Decode | Self::UnsupportedFormat | Self::TooLarge | Self::InvalidName
        )
    }
}

impl From<&RenderError> for ErrorKind {
    fn from(error: &RenderError) -> Self {
        match error {
            RenderError::UnsupportedFormat(_) => Self::UnsupportedFormat,
            RenderError::Io { .. } => Self::Io,
            RenderError::Decode { .. } => Self::Decode,
            RenderError::TooLarge { .. } => Self::TooLarge,
            RenderError::Panicked { .. } => Self::Panicked,
            RenderError::Timeout { .. } => Self::Timeout,
            RenderError::Cancelled { .. } => Self::Cancelled,
        }
    }
}

/// A failed file: the kind for the error card and a text for the log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThumbError {
    /// The kind of failure.
    pub kind: ErrorKind,
    /// The error text.
    pub message: String,
}

/// One finished file, sent to the UI.
#[derive(Debug)]
pub struct ThumbResult {
    /// The generation of the `open_folder` call that made the job.
    pub generation: u64,
    /// The item key: the source path.
    pub key: PathBuf,
    /// The 320 px thumbnail as RGBA8, or the failure.
    pub outcome: Result<RgbaImage, ThumbError>,
}

/// One unit of work for a worker.
#[derive(Debug, Clone)]
pub(super) struct Job {
    pub(super) generation: u64,
    pub(super) path: PathBuf,
    /// The source stamp from the scan.
    pub(super) stamp: SourceStamp,
    /// The stamp of the last render, when the row has cache paths.
    pub(super) stored: Option<SourceStamp>,
    /// The source folder name that keys the cache directory.
    pub(super) folder: String,
    /// The stable id of the source folder.
    pub(super) uuid: String,
    /// A failure seen earlier for this exact stamp. It is not rendered again.
    pub(super) known_failure: Option<ThumbError>,
    /// The colour tag config, or `None` when colour tagging is off.
    pub(super) colors: Option<ColorTagConfig>,
    /// True when a cache hit must still extract colours, because the path has
    /// no `auto_color` tag yet. A render always extracts.
    pub(super) needs_colors: bool,
}

/// The colour tag state read once per open.
#[derive(Debug, Default)]
pub(super) struct ColorState {
    config: Option<ColorTagConfig>,
    colored: HashSet<String>,
}

impl ColorState {
    /// Read the config and the paths that already have `auto_color` tags.
    pub(super) fn read(db: &Database) -> Self {
        let config = colortags::read_config(db);
        let colored = if config.is_some() {
            db.auto_colored_paths().unwrap_or_else(|error| {
                tracing::warn!("cannot list the coloured paths: {error}");
                HashSet::new()
            })
        } else {
            HashSet::new()
        };
        Self { config, colored }
    }

    /// The `colors` and `needs_colors` fields for the job of `path`.
    pub(super) fn for_path(&self, path: &str) -> (Option<ColorTagConfig>, bool) {
        (self.config, !self.colored.contains(path))
    }
}
