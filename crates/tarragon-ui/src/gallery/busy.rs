//! The busy strip under the filter bar.

use std::path::{Path, PathBuf};

use egui::Ui;

use super::Gallery;

/// The label that the strip shows while work runs.
const LABEL: &str = "Working...";

/// The busy state of the gallery.
#[derive(Clone, Debug, Default)]
pub(super) struct Busy {
    /// Whether work runs and the strip shows.
    working: bool,
    /// The folder that opens now. The gallery still holds the old folder's
    /// items, so the info bar names this one without taking it early.
    loading: Option<PathBuf>,
}

impl Busy {
    /// The folder that opens now, if one does.
    pub(super) fn loading(&self) -> Option<&Path> {
        self.loading.as_deref()
    }
}

impl Gallery {
    /// Name the folder that opens now in the info bar, or `None` when no open
    /// runs. The old folder shows again by itself when an open fails.
    pub fn set_loading_folder(&mut self, folder: Option<PathBuf>) {
        self.busy.loading = folder;
    }

    /// Show or hide the busy strip. The strip takes no space while hidden.
    pub fn set_busy(&mut self, busy: bool) {
        self.busy.working = busy;
    }

    /// Draw the busy strip when work runs, and nothing otherwise.
    pub(super) fn show_busy(&self, ui: &mut Ui) {
        if self.busy.working {
            crate::busy::strip(ui, LABEL);
        }
    }
}
