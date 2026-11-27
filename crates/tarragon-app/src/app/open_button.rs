//! The Preview's open buttons: for one selected file the apps it can open
//! in, the button text, and the clicks on it; for many the app they all open
//! in, when it is one.

use std::collections::HashSet;
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
    /// For a multi-selection, one file of each extension and the way it
    /// opens first.
    samples: Vec<(PathBuf, Choice)>,
}

impl OpenState {
    /// Look up default app names through `names`.
    pub const fn new(names: AppNames) -> Self {
        Self {
            names,
            file: None,
            choices: Vec::new(),
            samples: Vec::new(),
        }
    }

    /// Follow the selection. Returns the button to show for one file. A
    /// failed template read leaves only the default app.
    pub fn select(&mut self, db: &Database, selected: &[PathBuf]) -> Option<OpenButton> {
        let file = match selected {
            [file] => Some(file.as_path()),
            _ => None,
        };
        self.file = file.map(Path::to_path_buf);
        self.choices = file.map_or_else(Vec::new, |file| choices_of(db, file));
        self.samples = if selected.len() > 1 {
            // One file stands for its extension, as the many-file opener
            // looks up each extension once.
            let mut seen = HashSet::new();
            selected
                .iter()
                .filter(|path| seen.insert(extension_key(path)))
                .filter_map(|path| {
                    let first = choices_of(db, path).into_iter().next()?;
                    Some((path.clone(), first))
                })
                .collect()
        } else {
            Vec::new()
        };
        self.button()
    }

    /// The app every file of a multi-selection opens in, when it is one
    /// named app. A default app whose name is not known yet gives `None`.
    pub fn open_all_app(&mut self) -> Option<String> {
        let samples = std::mem::take(&mut self.samples);
        let mut names = samples.iter().map(|(path, choice)| match choice {
            Choice::Template { name, .. } => Some(name.clone()),
            Choice::Default => self.names.name_for(path).map(str::to_owned),
        });
        let first = names.next().flatten();
        let one = first.filter(|first| names.all(|name| name.as_ref() == Some(first)));
        self.samples = samples;
        one
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

/// The ways to open `file`. A failed template read leaves only the default
/// app.
fn choices_of(db: &Database, file: &Path) -> Vec<Choice> {
    choices::choices(db, file).unwrap_or_else(|error| {
        tracing::warn!("cannot read the editor rows: {error}");
        vec![Choice::Default]
    })
}

/// The lower-case extension of `path`, empty when it has none.
fn extension_key(path: &Path) -> String {
    path.extension()
        .map(|ext| ext.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests;
