//! The busy flag of the gallery.

use super::App;

impl App {
    /// Give the gallery the busy state of the pipeline.
    ///
    /// It runs every frame, after the open ends are taken, so the strip shows
    /// from the request to the last end and not only while one open runs.
    pub(super) fn sync_busy(&mut self) {
        self.shell.gallery_mut().set_busy(self.thumbs.is_busy());
    }
}

#[cfg(test)]
#[path = "busy_tests.rs"]
mod tests;
