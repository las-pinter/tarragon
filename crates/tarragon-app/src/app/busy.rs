//! The running jobs for the status bar, and the folder that opens.

use std::path::Path;

use super::App;

impl App {
    /// Give the status bar the running jobs and the gallery header the
    /// folder that opens.
    ///
    /// It runs every frame, after the open ends are taken, so a job shows
    /// from the request to its end. The folder label ends with the open, so a
    /// failed open shows the old folder.
    pub(super) fn sync_busy(&mut self) {
        let loading = self.thumbs.loading_folder().map(Path::to_path_buf);
        let jobs = self.thumbs.jobs();
        self.shell.set_jobs(jobs);
        self.shell.gallery_mut().set_loading_folder(loading);
    }
}

#[cfg(test)]
#[path = "busy_tests.rs"]
mod tests;
