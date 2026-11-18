//! The menu choices that write files: rename, duplicate, and paste.
//!
//! A rename of one file runs on the UI thread. Duplicate and paste run on a
//! short-lived thread, one at a time, so many files and a slow clipboard cannot
//! freeze the window. After a write the shown folder loads again and the new
//! files are selected once the new list is in the gallery.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use crossbeam_channel::TryRecvError;
use eframe::egui::Context;
use tarragon_cache::store::CacheStore;
use tarragon_core::format;
use tarragon_db::Database;
use tarragon_ui::gallery::Scope;

use crate::app::App;
use crate::clipboard::{self, ClipboardError};
use crate::fileops::{self, Copied, FileOpError};

use super::undo::{UndoChange, UndoStep};

/// What one file write changed.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(super) struct FileWrite {
    /// The files that a duplicate or a paste made.
    pub copied: Vec<Copied>,
    /// The old and the new path of a renamed file.
    pub renamed: Option<(PathBuf, PathBuf)>,
}

/// What the write thread answers: the write and the notice to show, if any.
#[derive(Default)]
pub(super) struct Outcome {
    /// What changed on disk.
    pub write: FileWrite,
    /// The text for the notice line.
    pub notice: Option<String>,
    /// The folder the write was for. The shown folder loads again only when it
    /// is still this folder at the end of the write.
    pub folder: Option<PathBuf>,
    /// The files a delete removed.
    pub deleted: Vec<PathBuf>,
    /// The files the trash refused, to offer a permanent delete.
    pub refused: Vec<PathBuf>,
    /// What the write does to the undo slot.
    pub undo: UndoChange,
}

/// Files to select after the shown folder has loaded again.
pub(super) struct PendingSelect {
    /// The folder the gallery showed when the write ended.
    folder: Option<PathBuf>,
    /// The files to select, or the files that were removed when `near` is set.
    paths: Vec<PathBuf>,
    /// Select the tile that took the place of the removed files.
    near: bool,
}

/// A reload that waits until the open that runs now has ended.
pub(super) struct Deferred {
    /// The folder the write was for. A library reload has the shown folder.
    target: Option<PathBuf>,
    /// The files to hold, as for [`PendingSelect`].
    paths: Vec<PathBuf>,
    /// Select near the removed files, as for [`PendingSelect`].
    near: bool,
}

impl App {
    /// Rename the file to `new_stem` and select it under the new name. An
    /// error shows a notice, and nothing changes.
    pub(super) fn rename_to(&mut self, ctx: &Context, path: &Path, new_stem: &str) {
        if self.menu.writing.is_some() {
            self.notify(ctx, BUSY.to_owned());
            return;
        }
        let cache = CacheStore::new(self.cache_root.clone());
        match rename_write(&self.db, &cache, path, new_stem) {
            Ok(write) => {
                if let Some((old, new)) = write.renamed.clone() {
                    self.set_undo(Some(UndoStep::Rename { old, new }));
                }
                self.finish_write(ctx, &write, None, path.parent());
            }
            Err(error) => {
                tracing::warn!("cannot rename the file: {error}");
                self.notify(ctx, format!("Cannot rename: {error}"));
            }
        }
    }

    /// Copy each file next to itself, on the write thread.
    ///
    /// In the library view (`Scope::All`) each file is copied into its own
    /// folder, and the library loads again at the end.
    pub(super) fn duplicate(&mut self, ctx: &Context, paths: Vec<PathBuf>) {
        let folder = self.shell.gallery_mut().folder().map(Path::to_path_buf);
        self.start_write(ctx, move || duplicate_outcome(&paths, folder));
    }

    /// Paste the files of the clipboard into the shown folder, on the write
    /// thread. It does nothing without a shown folder. In the library view
    /// (`Scope::All`) it also does nothing, because no one folder is the target
    /// there.
    pub(super) fn paste(&mut self, ctx: &Context) {
        self.paste_with(ctx, clipboard::read_files);
    }

    /// [`Self::paste`] with the call that reads the clipboard, so a test needs
    /// no clipboard program.
    pub(super) fn paste_with(
        &mut self,
        ctx: &Context,
        read: impl FnOnce() -> Result<Vec<PathBuf>, ClipboardError> + Send + 'static,
    ) {
        let gallery = self.shell.gallery_mut();
        if gallery.scope() != Scope::Folder {
            return;
        }
        let Some(folder) = gallery.folder().map(Path::to_path_buf) else {
            return;
        };
        self.start_write(ctx, move || paste_outcome(&folder, read));
    }

    /// Run `job` on a thread, unless a write still runs. The notice of a
    /// second request says so, and the first write goes on.
    ///
    /// The thread is detached: no code joins it, and the app keeps only the
    /// receiver of its answer. When the app exits during a copy, the process
    /// ends and the thread stops where it is, with no clean-up. A file that was
    /// half written stays on disk, because [`fileops::copy_into`] removes a
    /// partial file only when the copy returns an error. No answer is read, and
    /// the database is not changed for that file.
    pub(super) fn start_write(
        &mut self,
        ctx: &Context,
        job: impl FnOnce() -> Outcome + Send + 'static,
    ) {
        if self.menu.writing.is_some() {
            self.notify(ctx, BUSY.to_owned());
            return;
        }
        let (sender, receiver) = crossbeam_channel::bounded(1);
        let wake = ctx.clone();
        let spawned = std::thread::Builder::new()
            .name("tarragon-files".to_owned())
            .spawn(move || {
                // A closed window drops the receiver, so the answer is not needed.
                let _ = sender.send(job());
                wake.request_repaint();
            });
        match spawned {
            Ok(_) => self.menu.writing = Some(receiver),
            Err(error) => {
                tracing::warn!("cannot start the file thread: {error}");
                self.notify(ctx, format!("Cannot write files: {error}"));
            }
        }
    }

    /// Take the answer of the write thread, when it is ready.
    pub(super) fn poll_write(&mut self, ctx: &Context) {
        let Some(receiver) = self.menu.writing.take() else {
            return;
        };
        match receiver.try_recv() {
            Ok(outcome) => {
                let folder = outcome.folder.as_deref();
                self.apply_undo_change(outcome.undo);
                self.finish_write(ctx, &outcome.write, outcome.notice, folder);
                self.finish_delete(ctx, outcome.deleted, outcome.refused, folder);
            }
            Err(TryRecvError::Empty) => self.menu.writing = Some(receiver),
            Err(TryRecvError::Disconnected) => {
                tracing::warn!("the file thread ended without an answer");
                self.notify(ctx, "The file task ended without an answer".to_owned());
            }
        }
    }

    /// Show the notice, and load the shown folder again when a file changed.
    /// `folder` is the folder the write was for.
    pub(super) fn finish_write(
        &mut self,
        ctx: &Context,
        write: &FileWrite,
        notice: Option<String>,
        folder: Option<&Path>,
    ) {
        if let Some(text) = notice {
            self.notify(ctx, text);
        }
        let mut changed: Vec<PathBuf> = write.copied.iter().map(|c| c.path.clone()).collect();
        if let Some((_, new)) = &write.renamed {
            changed.push(new.clone());
        }
        if !changed.is_empty() {
            self.reload_and_select(ctx, changed, folder);
        }
    }

    /// Load the shown folder, or the library, again and hold `paths` until the
    /// new list is in the gallery. Nothing happens, and nothing is held, when
    /// the shown folder is not `target`: a switch to another folder must win
    /// over this reload. While another open is pending or running, the reload
    /// waits for it.
    fn reload_and_select(&mut self, ctx: &Context, paths: Vec<PathBuf>, target: Option<&Path>) {
        self.reload_and_hold(ctx, paths, false, target);
    }

    /// [`Self::reload_and_select`], or with `near` set, hold the removed `paths`
    /// and select the tile near their place when the new list lacks them.
    pub(super) fn reload_and_hold(
        &mut self,
        ctx: &Context,
        paths: Vec<PathBuf>,
        near: bool,
        target: Option<&Path>,
    ) {
        if self.thumbs.is_opening() {
            // The open may have listed the folder before this write, so the
            // reload waits for the open to end. A newer request replaces it.
            self.menu.deferred = Some(Deferred {
                target: target.map(Path::to_path_buf),
                paths,
                near,
            });
            return;
        }
        self.menu.deferred = None;
        let gallery = self.shell.gallery_mut();
        let folder = gallery.folder().map(Path::to_path_buf);
        match (gallery.scope(), &folder) {
            (Scope::Folder, Some(shown)) if Some(shown.as_path()) == target => {
                self.thumbs.reload_folder(shown.clone());
            }
            (Scope::All, _) => self.thumbs.reload_library(),
            _ => return,
        }
        self.menu.pending = Some(PendingSelect {
            folder,
            paths,
            near,
        });
        ctx.request_repaint();
    }

    /// Run the reload that waited for an open, once no open is pending or
    /// running. The reload is dropped when the shown folder is not its folder
    /// any more: a switch to another folder wins.
    pub(super) fn apply_deferred_reload(&mut self, ctx: &Context) {
        if self.thumbs.is_opening() {
            return;
        }
        if let Some(deferred) = self.menu.deferred.take() {
            self.reload_and_hold(
                ctx,
                deferred.paths,
                deferred.near,
                deferred.target.as_deref(),
            );
        }
    }

    /// Select the held files once the gallery has them. The hold ends without a
    /// selection when the user opened another folder, or when the load ended
    /// and the files are not in the list.
    pub(super) fn apply_pending_select(&mut self, ctx: &Context) {
        let Some(pending) = self.menu.pending.take() else {
            return;
        };
        let gallery = self.shell.gallery_mut();
        if gallery.folder() != pending.folder.as_deref() {
            return;
        }
        let arrived = pending.paths.iter().any(|path| gallery.has_item(path));
        if pending.near {
            // Removed files: the new list has arrived when none of them is in it.
            if !arrived {
                gallery.select_near();
                ctx.request_repaint();
            } else if self.thumbs.is_opening() {
                self.menu.pending = Some(pending);
            }
        } else if arrived {
            gallery.select_paths(&pending.paths);
            ctx.request_repaint();
        } else if self.thumbs.is_opening() {
            self.menu.pending = Some(pending);
        }
    }
}

/// The notice for a second write request.
pub(super) const BUSY: &str = "Another file task is running. Try again soon.";

/// Rename the file on disk and in the database.
pub(super) fn rename_write(
    db: &Database,
    cache: &CacheStore,
    path: &Path,
    new_stem: &str,
) -> Result<FileWrite, FileOpError> {
    let new = fileops::rename(db, cache, path, new_stem)?;
    Ok(FileWrite {
        copied: Vec::new(),
        renamed: Some((path.to_path_buf(), new)),
    })
}

/// Copy each file into `folder`, or next to itself when `folder` is none. Give
/// the files made, and the first error with the count of failures.
fn copy_all(paths: &[PathBuf], folder: Option<&Path>) -> (Vec<Copied>, Option<(String, usize)>) {
    let mut copied = Vec::new();
    let mut failed = None::<(String, usize)>;
    for path in paths {
        let target = folder.or_else(|| path.parent()).unwrap_or(Path::new(""));
        match fileops::copy_into(path, target) {
            Ok(done) => copied.push(done),
            Err(error) => {
                tracing::warn!("cannot copy the file: {error}");
                match &mut failed {
                    Some((_, count)) => *count += 1,
                    None => failed = Some((error.to_string(), 1)),
                }
            }
        }
    }
    (copied, failed)
}

/// Copy each file next to itself. The notice shows only a failure.
fn duplicate_outcome(paths: &[PathBuf], folder: Option<PathBuf>) -> Outcome {
    let (copied, failed) = copy_all(paths, None);
    let notice = failed.map(|(first, count)| {
        if paths.len() <= 1 {
            format!("Cannot duplicate: {first}")
        } else {
            format!(
                "Cannot duplicate: {first} ({count} of {} files failed)",
                paths.len()
            )
        }
    });
    // A duplicate that made no copy leaves an older step in place.
    let undo = if copied.is_empty() {
        UndoChange::Keep
    } else {
        UndoChange::Set(UndoStep::Duplicate {
            copies: copied.clone(),
        })
    };
    Outcome {
        write: FileWrite {
            copied,
            renamed: None,
        },
        notice,
        folder,
        undo,
        ..Outcome::default()
    }
}

/// Copy the files that `read` gives into `folder`. Only regular files of a
/// supported type are copied. The notice counts the pasted and the skipped
/// files.
fn paste_outcome(
    folder: &Path,
    read: impl FnOnce() -> Result<Vec<PathBuf>, ClipboardError>,
) -> Outcome {
    let listed = match read() {
        Ok(listed) => listed,
        Err(error) => {
            return Outcome {
                write: FileWrite::default(),
                notice: Some(paste_failure_text(&error)),
                folder: Some(folder.to_path_buf()),
                undo: UndoChange::Clear,
                ..Outcome::default()
            };
        }
    };
    if listed.is_empty() {
        return Outcome {
            write: FileWrite::default(),
            notice: Some("The clipboard holds no files".to_owned()),
            folder: Some(folder.to_path_buf()),
            undo: UndoChange::Clear,
            ..Outcome::default()
        };
    }
    let kept: Vec<PathBuf> = listed
        .iter()
        .filter(|path| path.is_file() && format::is_supported(path))
        .cloned()
        .collect();
    let skipped = listed.len() - kept.len();
    let (copied, failed) = copy_all(&kept, Some(folder));
    let notice = paste_text(copied.len(), skipped, failed);
    Outcome {
        write: FileWrite {
            copied,
            renamed: None,
        },
        notice: Some(notice),
        folder: Some(folder.to_path_buf()),
        undo: UndoChange::Clear,
        ..Outcome::default()
    }
}

/// The notice for the end of a paste.
fn paste_text(pasted: usize, skipped: usize, failed: Option<(String, usize)>) -> String {
    let noun = if pasted == 1 { "file" } else { "files" };
    let mut text = format!("Pasted {pasted} {noun}");
    if skipped > 0 {
        let _ = write!(text, ", skipped {skipped}");
    }
    if let Some((first, count)) = failed {
        let _ = write!(text, ", {count} failed: {first}");
    }
    text
}

/// The notice for a clipboard that could not be read.
fn paste_failure_text(error: &ClipboardError) -> String {
    match error {
        ClipboardError::ToolMissing(package) => format!("Install {package} to paste files"),
        other => format!("Cannot paste files: {other}"),
    }
}

#[cfg(test)]
#[path = "writes_tests.rs"]
mod tests;
