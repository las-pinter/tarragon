//! Opening the image that the gallery queued, in its editor.

use super::App;
use crate::editor;

impl App {
    /// Open the queued image in its editor, if the gallery queued one.
    ///
    /// The take runs every frame because a newer request replaces one that was
    /// not taken. A failed start shows a notice and is not retried, since the
    /// request is gone and a retry on each frame would start the program again
    /// and again.
    pub(super) fn apply_open_request(&mut self) {
        let Some(path) = self.shell.take_open_request() else {
            return;
        };
        if let Err(error) = editor::open(&self.db, &path) {
            self.notice_failure("cannot open the file in an editor", &error);
        }
    }

    /// Log a failed start and show its text in the notice line, as the menu
    /// path does, and ask for a frame so the notice shows.
    pub(super) fn notice_failure(&mut self, what: &str, error: &editor::OpenError) {
        tracing::warn!("{what}: {error}");
        self.shell.gallery_mut().notify(error.to_string());
        if let Some(ctx) = self.repaint_ctx.get() {
            ctx.request_repaint();
        }
    }
}

#[cfg(all(test, unix))]
#[path = "open_tests.rs"]
mod tests;
