//! Opening the image that the gallery queued, in its editor.

use super::App;
use crate::editor;

impl App {
    /// Open the queued image in its editor, if the gallery queued one.
    ///
    /// The take runs every frame because a newer request replaces one that was
    /// not taken. A failed start is logged and not retried, since the request
    /// is gone and a retry on each frame would start the program again and
    /// again.
    pub(super) fn apply_open_request(&mut self) {
        let Some(path) = self.shell.take_open_request() else {
            return;
        };
        if let Err(error) = editor::open(&self.db, &path) {
            tracing::warn!("cannot open the file in an editor: {error}");
        }
    }
}

#[cfg(all(test, unix))]
#[path = "open_tests.rs"]
mod tests;
