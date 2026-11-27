//! The error card of the Preview panel: it shows for the one selected file
//! that failed to render, and its buttons act on that file.

use std::collections::HashSet;
use std::path::Path;

use eframe::egui::Context;
use tarragon_ui::preview::{MultiSummary, PreviewEvent, PreviewFailure};

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

    /// Give the Preview the open button of one file and the summary of many.
    pub(super) fn show_open_buttons(&mut self) {
        let button = self.open_button.button();
        let app = self.open_button.open_all_app();
        let bytes = self.shell.selected_bytes();
        let preview = self.shell.preview_mut();
        preview.set_open(button);
        preview.set_summary(MultiSummary { bytes, app });
    }

    /// Act on the card buttons, the open button for the one selected file,
    /// and the open-all button.
    pub(super) fn apply_card_events(&mut self, ctx: &Context, events: &[PreviewEvent]) {
        if events.contains(&PreviewEvent::OpenAll) {
            let paths = self.shell.selected_paths();
            if !paths.is_empty() {
                self.open_in_editor(ctx, paths);
            }
        }
        for event in events {
            let Some(path) = self.shell.sole_selected().map(Path::to_path_buf) else {
                return;
            };
            match event {
                PreviewEvent::RetryRender => self.thumbs.retry(&HashSet::from([path])),
                PreviewEvent::OpenInEditor => {
                    if let Err(error) = editor::open(&self.db, &path) {
                        self.notice_failure("cannot open the file in an editor", &error);
                    }
                }
                _ => {}
            }
        }
        let opener = self.opener_path.as_deref();
        if let Some(error) = self.open_button.apply(events, opener) {
            self.notice_failure("cannot open the file", &error);
        }
    }
}
