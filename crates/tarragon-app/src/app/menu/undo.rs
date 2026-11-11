//! One-step undo for the last Rename or Duplicate of this session.
//!
//! The app keeps one step. The next Rename or Duplicate replaces it, and Paste
//! and every delete clear it, so Ctrl+Z never reverses an older step behind a
//! newer change. A run of undo clears the step, and nothing redoes it. The
//! step is told to the gallery each time it changes, because the shell takes
//! the Ctrl+Z press only while a step exists.

use std::collections::HashSet;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use eframe::egui::Context;
use tarragon_cache::store::CacheStore;
use tarragon_core::file_name::split_file_name;

use super::delete::Failures;
use super::writes::{BUSY, Outcome, rename_write};
use crate::app::App;
use crate::fileops::{self, Copied};

/// What Ctrl+Z reverses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum UndoStep {
    /// A rename: move the file at `new` back to `old`.
    Rename {
        /// The path before the rename.
        old: PathBuf,
        /// The path after the rename.
        new: PathBuf,
    },
    /// A duplicate: send the copies to the trash.
    Duplicate {
        /// The files the duplicate made, with their length and time at creation.
        copies: Vec<Copied>,
    },
}

/// What a write does to the undo slot when it ends.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(super) enum UndoChange {
    /// Leave the slot as it is.
    #[default]
    Keep,
    /// Empty the slot.
    Clear,
    /// Replace the slot with a step.
    Set(UndoStep),
}

impl App {
    /// Apply what a write says about the undo slot.
    pub(super) fn apply_undo_change(&mut self, change: UndoChange) {
        match change {
            UndoChange::Keep => {}
            UndoChange::Clear => self.set_undo(None),
            UndoChange::Set(step) => self.set_undo(Some(step)),
        }
    }

    /// Fill or empty the slot, and tell the gallery which it is.
    pub(super) fn set_undo(&mut self, step: Option<UndoStep>) {
        self.shell.gallery_mut().set_undo_available(step.is_some());
        self.menu.undo = step;
    }

    /// Undo the step in the slot, with the real trash for a duplicate.
    pub(super) fn undo_last(&mut self, ctx: &Context) {
        self.undo_with(ctx, fileops::trash);
    }

    /// Undo the step in the slot. A test passes its own `remove`, so no test
    /// reaches the real trash.
    ///
    /// A rename moves back at once, as a rename does. A duplicate trashes its
    /// copies on the write thread. While a write runs, the call shows a notice
    /// and keeps the step.
    pub(super) fn undo_with(
        &mut self,
        ctx: &Context,
        remove: impl FnOnce(&[PathBuf]) -> Failures + Send + 'static,
    ) {
        if self.menu.writing.is_some() {
            self.notify(ctx, BUSY.to_owned());
            return;
        }
        match self.menu.undo.clone() {
            None => {}
            Some(UndoStep::Rename { old, new }) => self.undo_rename(ctx, &old, &new),
            Some(UndoStep::Duplicate { copies }) => {
                self.start_write(ctx, move || undo_duplicate_outcome(&copies, remove));
            }
        }
    }

    /// Move the file at `new` back to `old`. A refusal empties the slot. A
    /// failure keeps it, so the user can try again.
    fn undo_rename(&mut self, ctx: &Context, old: &Path, new: &Path) {
        let stem = old
            .file_name()
            .and_then(|name| name.to_str())
            .map(|name| split_file_name(name).0.to_owned());
        let Some(stem) = stem else {
            self.set_undo(None);
            self.notify(ctx, "Cannot undo the rename: no usable old name".to_owned());
            return;
        };
        if let Some(reason) = rename_refusal(old, new) {
            self.set_undo(None);
            self.notify(ctx, format!("Cannot undo the rename: {reason}"));
            return;
        }
        let cache = CacheStore::new(self.cache_root.clone());
        match rename_write(&self.db, &cache, new, &stem) {
            Ok(write) => {
                self.set_undo(None);
                let text = format!("Undid the rename: {}", name_of(old));
                self.finish_write(ctx, &write, Some(text), old.parent());
            }
            Err(error) => {
                tracing::warn!("cannot undo the rename: {error}");
                self.notify(ctx, format!("Cannot undo the rename: {error}"));
            }
        }
    }
}

/// Why a rename cannot be undone, or none when it can: the renamed file is
/// gone, or something else has the old name now.
fn rename_refusal(old: &Path, new: &Path) -> Option<String> {
    if new.symlink_metadata().is_err() {
        Some(format!("{} is gone", name_of(new)))
    } else if old.symlink_metadata().is_ok() {
        Some(format!("{} is taken", name_of(old)))
    } else {
        None
    }
}

/// The file name of `path` for a notice.
fn name_of(path: &Path) -> String {
    path.file_name()
        .map_or_else(String::new, |name| name.to_string_lossy().into_owned())
}

/// Trash each copy that is still as the duplicate made it, with `remove`. A
/// changed or missing copy stays. A trash failure keeps the slot, and a run
/// with no failure empties it.
fn undo_duplicate_outcome(
    copies: &[Copied],
    remove: impl FnOnce(&[PathBuf]) -> Failures,
) -> Outcome {
    let (intact, changed): (Vec<&Copied>, Vec<&Copied>) =
        copies.iter().partition(|copy| copy.is_unchanged());
    let paths: Vec<PathBuf> = intact.iter().map(|copy| copy.path.clone()).collect();
    let failures = if paths.is_empty() {
        Vec::new()
    } else {
        remove(&paths)
    };
    for (path, error) in &failures {
        tracing::warn!("cannot trash {}: {error}", path.display());
    }
    let failed: HashSet<&Path> = failures.iter().map(|(path, _)| path.as_path()).collect();
    let trashed: Vec<PathBuf> = paths
        .iter()
        .filter(|path| !failed.contains(path.as_path()))
        .cloned()
        .collect();
    let first = failures.first().map(|(_, error)| error.to_string());
    let notice = undo_duplicate_text(
        trashed.len(),
        changed.len(),
        failures.len(),
        first.as_deref(),
    );
    let undo = if failures.is_empty() {
        UndoChange::Clear
    } else {
        UndoChange::Keep
    };
    Outcome {
        notice: Some(notice),
        folder: copies
            .first()
            .and_then(|copy| copy.path.parent())
            .map(Path::to_path_buf),
        deleted: trashed,
        undo,
        ..Outcome::default()
    }
}

/// The notice for the end of a duplicate undo: how many copies went to the
/// trash, how many stayed because they changed, and the failures.
fn undo_duplicate_text(trashed: usize, kept: usize, failed: usize, first: Option<&str>) -> String {
    let noun = if trashed == 1 { "copy" } else { "copies" };
    let mut text = format!("Undid the duplicate: moved {trashed} {noun} to the Trash");
    if kept > 0 {
        let _ = write!(text, ", kept {kept} that changed or are gone");
    }
    if let Some(first) = first {
        let _ = write!(text, ", {failed} failed: {first}");
    }
    text
}

#[cfg(test)]
#[path = "undo_tests.rs"]
mod tests;
