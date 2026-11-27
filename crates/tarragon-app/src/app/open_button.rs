//! The Preview's open button for the one selected file: the apps it can
//! open in, the button text, and the clicks on it.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use tarragon_db::Database;
use tarragon_ui::preview::{OpenButton, PreviewEvent};

use crate::app_names::AppNames;
use crate::editor::OpenError;
use crate::editor::choices::{self, Choice, DEFAULT_APP};

/// The text of the button when the app name is not known.
pub const OPEN_TEXT: &str = "Open";

/// The open button state of the selection.
pub struct OpenState {
    /// The names of the default apps, looked up off the frame loop.
    names: AppNames,
    /// The file the button opens, when exactly one is selected.
    file: Option<PathBuf>,
    /// The ways to open that file, the button's own first.
    choices: Vec<Choice>,
}

impl OpenState {
    /// Look up default app names through `names`.
    pub const fn new(names: AppNames) -> Self {
        Self {
            names,
            file: None,
            choices: Vec::new(),
        }
    }

    /// Follow the selection: `file` is the one selected file, or `None`.
    /// Returns the button to show. A failed template read leaves only the
    /// default app.
    pub fn select(&mut self, db: &Database, file: Option<&Path>) -> Option<OpenButton> {
        self.file = file.map(Path::to_path_buf);
        self.choices = match file {
            Some(file) => choices::choices(db, file).unwrap_or_else(|error| {
                tracing::warn!("cannot read the editor rows: {error}");
                vec![Choice::Default]
            }),
            None => Vec::new(),
        };
        self.button()
    }

    /// Take the app names that arrived. Returns whether any did, so the
    /// caller can show the new [`button`](Self::button).
    pub fn poll(&mut self) -> bool {
        self.names.poll()
    }

    /// The button for the current file, or `None` with no single file.
    pub fn button(&mut self) -> Option<OpenButton> {
        let file = self.file.clone()?;
        let default_name = self.names.name_for(&file).map(str::to_owned);
        let label_of = |choice: &Choice| match (choice, &default_name) {
            (Choice::Default, Some(name)) | (Choice::Template { name, .. }, _) => name.clone(),
            (Choice::Default, None) => DEFAULT_APP.to_owned(),
        };
        let first = self.choices.first()?;
        let label = match (first, &default_name) {
            (Choice::Default, None) => OPEN_TEXT.to_owned(),
            _ => format!("Open in {}", label_of(first)),
        };
        let mut choices: Vec<String> = self.choices.iter().map(label_of).collect();
        // The default app is the last row; when an editor row already names
        // the same app, it is shown once. The rows before it keep their index.
        if let Some((last, earlier)) = choices.split_last()
            && earlier.contains(last)
        {
            choices.pop();
        }
        Some(OpenButton { label, choices })
    }

    /// Act on the open events. `path` is the `PATH` for the started app, or
    /// none to inherit the process one. Returns the first failure, for the
    /// notice.
    pub fn apply(&self, events: &[PreviewEvent], path: Option<&OsStr>) -> Option<OpenError> {
        let file = self.file.as_deref()?;
        events.iter().find_map(|event| {
            let index = match event {
                PreviewEvent::Open => 0,
                PreviewEvent::OpenWith(index) => *index,
                _ => return None,
            };
            let choice = self.choices.get(index)?;
            choices::open_with_path(file, choice, path).err()
        })
    }
}

#[cfg(test)]
mod tests;
