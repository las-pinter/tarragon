//! The busy flag of the gallery.

use std::path::Path;

use super::App;

impl App {
    /// Give the gallery the busy state of the pipeline and the folder that
    /// opens.
    ///
    /// It runs every frame, after the open ends are taken, so the strip shows
    /// from the request to the last end and not only while one open runs. The
    /// folder label ends with the open, so a failed open shows the old folder.
    pub(super) fn sync_busy(&mut self) {
        let loading = self.thumbs.loading_folder().map(Path::to_path_buf);
        let gallery = self.shell.gallery_mut();
        gallery.set_busy(self.thumbs.is_busy());
        gallery.set_loading_folder(loading);
    }
}

#[cfg(test)]
#[path = "busy_tests.rs"]
mod tests;
