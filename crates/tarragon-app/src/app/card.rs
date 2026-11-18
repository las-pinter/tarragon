//! The error card of the Preview panel: it shows for the one selected file
//! that failed to render, and its buttons act on that file.

use std::collections::HashSet;
use std::path::Path;

use tarragon_ui::preview::{PreviewEvent, PreviewFailure};

use super::App;
use crate::editor;

impl App {
    /// Show the error card when the one selected file failed to render, and
    /// remove it otherwise. A card that stays after a good retry would hide the
    /// picture, so this runs every frame.
    pub(super) fn show_failure(&mut self) {
        let failure = self.shell.sole_selected().and_then(|path| {
            let error = self.thumbs.failure(path)?;
            let name = path.file_name().unwrap_or(path.as_os_str());
            Some(PreviewFailure {
                name: name.to_string_lossy().into_owned(),
                message: error.message,
            })
        });
        self.shell.preview_mut().set_failure(failure);
    }

    /// Act on the card buttons for the one selected file.
    pub(super) fn apply_card_events(&mut self, events: &[PreviewEvent]) {
        for event in events {
            let Some(path) = self.shell.sole_selected().map(Path::to_path_buf) else {
                return;
            };
            match event {
                PreviewEvent::RetryRender => self.thumbs.retry(&HashSet::from([path])),
                PreviewEvent::OpenInEditor => {
                    if let Err(error) = editor::open(&self.db, &path) {
                        tracing::warn!("cannot open the file in an editor: {error}");
                    }
                }
                _ => {}
            }
        }
    }
}
