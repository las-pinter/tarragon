//! The confirmed deletes: move files to the trash, or delete them for good.
//!
//! The dialogs only ask. Nothing here runs until a confirm event arrives, so a
//! permanent delete is never reachable without one. The disk work runs on the
//! write thread. After it the shown folder loads again, so the scan drops the
//! rows of the gone files and keeps their tags, and the tile near the removed
//! place is selected. A file the trash refuses goes to a second prompt that
//! offers a permanent delete for those files only.

use std::collections::HashSet;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use eframe::egui::Context;

use super::writes::Outcome;
use crate::app::App;
use crate::fileops::{self, FileOpError};

/// What a delete call gives back: the failure of each path it could not remove.
type Failures = Vec<(PathBuf, FileOpError)>;

impl App {
    /// Carry out a delete the user confirmed: the trash, or for good when
    /// `permanent` is set.
    pub(super) fn confirmed_delete(&mut self, ctx: &Context, paths: Vec<PathBuf>, permanent: bool) {
        if permanent {
            self.delete_with(ctx, paths, true, fileops::delete_permanently);
        } else {
            self.delete_with(ctx, paths, false, fileops::trash);
        }
    }

    /// Run `remove` on the files on the write thread. A test passes its own
    /// `remove`, so no test reaches the real trash.
    pub(super) fn delete_with(
        &mut self,
        ctx: &Context,
        paths: Vec<PathBuf>,
        permanent: bool,
        remove: impl FnOnce(&[PathBuf]) -> Failures + Send + 'static,
    ) {
        let folder = self.shell.gallery_mut().folder().map(Path::to_path_buf);
        self.start_write(ctx, move || {
            delete_outcome(&paths, permanent, folder, remove)
        });
    }

    /// After a delete: offer a permanent delete for the files the trash
    /// refused, then load the folder again and select near the removed tiles.
    pub(super) fn finish_delete(
        &mut self,
        ctx: &Context,
        deleted: Vec<PathBuf>,
        refused: Vec<PathBuf>,
        folder: Option<&Path>,
    ) {
        if !refused.is_empty() {
            self.shell.gallery_mut().open_delete_instead(refused);
            // The write ended after the draw of this frame.
            ctx.request_repaint();
        }
        if !deleted.is_empty() {
            // Before the new list drops the tiles.
            self.shell.gallery_mut().remember_place(&deleted);
            self.reload_and_hold(ctx, deleted, true, folder);
        }
    }
}

/// Remove the files with `remove` and say what happened. A refused trash file
/// goes to `refused`. A refused permanent delete only shows in the notice.
fn delete_outcome(
    paths: &[PathBuf],
    permanent: bool,
    folder: Option<PathBuf>,
    remove: impl FnOnce(&[PathBuf]) -> Failures,
) -> Outcome {
    let failures = remove(paths);
    for (path, error) in &failures {
        tracing::warn!("cannot remove {}: {error}", path.display());
    }
    let failed: HashSet<&Path> = failures.iter().map(|(path, _)| path.as_path()).collect();
    let deleted: Vec<PathBuf> = paths
        .iter()
        .filter(|path| !failed.contains(path.as_path()))
        .cloned()
        .collect();
    let first = failures.first().map(|(_, error)| error.to_string());
    let notice = delete_text(permanent, deleted.len(), failures.len(), first.as_deref());
    let refused = if permanent {
        Vec::new()
    } else {
        failures.into_iter().map(|(path, _)| path).collect()
    };
    Outcome {
        notice: Some(notice),
        folder,
        deleted,
        refused,
        ..Outcome::default()
    }
}

/// The notice for the end of a delete: how many went, how many failed, and the
/// first error.
fn delete_text(permanent: bool, done: usize, failed: usize, first: Option<&str>) -> String {
    let noun = if done == 1 { "file" } else { "files" };
    let mut text = if permanent {
        format!("Deleted {done} {noun}")
    } else {
        format!("Moved {done} {noun} to the Trash")
    };
    if let Some(first) = first {
        let _ = write!(text, ", {failed} failed: {first}");
    }
    text
}

#[cfg(test)]
#[path = "delete_tests.rs"]
mod tests;
