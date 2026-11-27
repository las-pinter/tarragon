//! The folder that opens, which the header names before the gallery holds
//! its files.

use std::path::{Path, PathBuf};

use super::Gallery;

/// The open state of the gallery.
#[derive(Clone, Debug, Default)]
pub(super) struct Loading {
    /// The folder that opens now. The gallery still holds the old folder's
    /// items, so the header names this one without taking it early.
    folder: Option<PathBuf>,
}

impl Loading {
    /// The folder that opens now, if one does.
    pub(super) fn folder(&self) -> Option<&Path> {
        self.folder.as_deref()
    }
}

impl Gallery {
    /// Name the folder that opens now in the header, or `None` when no open
    /// runs. The old folder shows again by itself when an open fails.
    pub fn set_loading_folder(&mut self, folder: Option<PathBuf>) {
        self.opening.folder = folder;
    }
}
