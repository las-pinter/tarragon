//! The busy strip under the filter bar.

use egui::Ui;

use super::Gallery;

/// The label that the strip shows while work runs.
const LABEL: &str = "Working...";

/// The busy state of the gallery.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Busy {
    /// Whether work runs and the strip shows.
    working: bool,
}

impl Gallery {
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
