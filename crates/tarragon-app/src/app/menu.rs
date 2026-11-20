//! The choices of the grid context menu that the app carries out.
//!
//! The gallery only raises the events. Work that can stall runs off the UI
//! thread: the clipboard copy on a short-lived thread, and the render again of
//! thumbnails on the work thread. Starting a program does not wait for it. The
//! choices that write files are in `writes`, the confirmed deletes in `delete`,
//! and the one-step undo in `undo`.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crossbeam_channel::{Receiver, TryRecvError};
use eframe::egui::Context;
use tarragon_ui::gallery::GridMenuEvent;

use super::App;
use crate::clipboard::{self, ClipboardError};
use crate::{editor, tags};
use undo::UndoStep;
use writes::{Deferred, Outcome, PendingSelect};

/// What a clipboard copy on its thread answers.
type CopyResult = Result<(), ClipboardError>;

/// What an open on its thread answers: the report and the file count.
type OpenAnswer = (editor::OpenReport, usize);

/// What the menu handlers keep between frames.
#[derive(Default)]
pub(super) struct MenuState {
    /// The answer of the running clipboard copy, until it arrives.
    copying: Option<Receiver<CopyResult>>,
    /// The answers of the opens that still run, one per open.
    opening: Vec<Receiver<OpenAnswer>>,
    /// The answer of the running duplicate or paste, until it arrives.
    writing: Option<Receiver<Outcome>>,
    /// The files to select once the shown folder has loaded again.
    pending: Option<PendingSelect>,
    /// A reload that waits for the open that runs now.
    deferred: Option<Deferred>,
    /// The one step that Ctrl+Z can undo. Change it only through
    /// `App::set_undo`, which tells the gallery.
    undo: Option<UndoStep>,
}

impl App {
    /// Take what the menu work has finished, then carry out the menu choices
    /// the gallery raised this frame. Runs once per frame, after the draw.
    pub(super) fn apply_menu_events(&mut self, ctx: &Context) {
        self.poll_menu_work(ctx);
        self.poll_write(ctx);
        self.apply_deferred_reload(ctx);
        self.apply_pending_select(ctx);
        for event in self.shell.gallery_mut().take_menu_events() {
            self.apply_menu_event(ctx, event);
        }
    }

    /// Carry out one menu choice.
    fn apply_menu_event(&mut self, ctx: &Context, event: GridMenuEvent) {
        match event {
            GridMenuEvent::Open(paths) => self.open_in_editor(ctx, paths),
            GridMenuEvent::ShowInFileManager(path) => self.show_in_file_manager(ctx, &path),
            GridMenuEvent::RegenerateThumbnails(paths) => self.regenerate(ctx, paths),
            GridMenuEvent::AddTag { pos, paths } => {
                self.shell.gallery_mut().open_tag_picker(pos, paths);
            }
            GridMenuEvent::CopyPath(path) => ctx.copy_text(path.to_string_lossy().into_owned()),
            GridMenuEvent::Copy(paths) => self.copy_files(ctx, paths),
            GridMenuEvent::Rename(path) => self.shell.gallery_mut().open_rename(path),
            GridMenuEvent::Delete(paths) => self.shell.gallery_mut().open_delete(paths, false),
            GridMenuEvent::DeletePermanently(paths) => {
                self.shell.gallery_mut().open_delete(paths, true);
            }
            GridMenuEvent::TagPaths { paths, tag } => self.tag_paths(ctx, &paths, &tag),
            GridMenuEvent::RenameTo { path, new_stem } => self.rename_to(ctx, &path, &new_stem),
            GridMenuEvent::Duplicate(paths) => self.duplicate(ctx, paths),
            GridMenuEvent::Paste => self.paste(ctx),
            GridMenuEvent::Undo => self.undo_last(ctx),
            GridMenuEvent::ConfirmedDelete { paths, permanent } => {
                self.confirmed_delete(ctx, paths, permanent);
            }
        }
    }

    /// Show a notice and ask for a frame, because the notice line was drawn
    /// before this call and egui repaints only on input.
    fn notify(&mut self, ctx: &Context, text: String) {
        self.shell.gallery_mut().notify(text);
        ctx.request_repaint();
    }

    /// Open the files in their editors from a thread, because the default
    /// app lookups start helper programs that can take seconds. Files that
    /// share an app go into one start when the app takes many files. No call
    /// waits for an editor. A failure shows one notice with the counts.
    fn open_in_editor(&mut self, ctx: &Context, paths: Vec<PathBuf>) {
        let (sender, receiver) = crossbeam_channel::bounded(1);
        let db = Arc::clone(&self.db);
        let wake = ctx.clone();
        let spawned = std::thread::Builder::new()
            .name("tarragon-open".to_owned())
            .spawn(move || {
                let report = editor::open_many(&db, &paths);
                // The app closed and dropped the receiver; nothing to show.
                let _ = sender.send((report, paths.len()));
                wake.request_repaint();
            });
        match spawned {
            Ok(_) => self.menu.opening.push(receiver),
            Err(error) => {
                tracing::warn!("cannot start the open thread: {error}");
                self.notify(ctx, format!("Cannot open the files: {error}"));
            }
        }
    }

    /// Show the folder of the file in the file manager.
    fn show_in_file_manager(&mut self, ctx: &Context, path: &Path) {
        if let Err(error) = editor::open_folder_in_manager(path) {
            tracing::warn!("cannot open the file manager: {error}");
            self.notify(ctx, error.to_string());
        }
    }

    /// Ask the work thread to render the files again. The notice comes when the
    /// work ends.
    fn regenerate(&mut self, ctx: &Context, paths: Vec<PathBuf>) {
        if !self.thumbs.regenerate(paths.into_iter().collect()) {
            self.notify(ctx, "Another task is waiting. Try again soon.".to_owned());
        }
    }

    /// Copy the files to the clipboard, unless a copy still runs. A second copy
    /// would replace the answer of the first, and its failure would go unseen.
    fn copy_files(&mut self, ctx: &Context, paths: Vec<PathBuf>) {
        if self.menu.copying.is_some() {
            self.notify(ctx, "Copy in progress".to_owned());
            return;
        }
        self.start_copy(ctx, paths, clipboard::copy_files);
    }

    /// Put the files on the clipboard from a thread, so a stuck helper program
    /// cannot freeze the window.
    fn start_copy(
        &mut self,
        ctx: &Context,
        paths: Vec<PathBuf>,
        copy: impl FnOnce(&[PathBuf]) -> CopyResult + Send + 'static,
    ) {
        let (sender, receiver) = crossbeam_channel::bounded(1);
        let wake = ctx.clone();
        let spawned = std::thread::Builder::new()
            .name("tarragon-copy".to_owned())
            .spawn(move || {
                // A newer copy dropped the receiver, so the answer is not needed.
                let _ = sender.send(copy(&paths));
                wake.request_repaint();
            });
        match spawned {
            Ok(_) => self.menu.copying = Some(receiver),
            Err(error) => {
                tracing::warn!("cannot start the copy thread: {error}");
                self.notify(ctx, format!("Cannot copy files: {error}"));
            }
        }
    }

    /// Add the user tag to the files in one batch.
    fn tag_paths(&mut self, ctx: &Context, paths: &[PathBuf], tag: &str) {
        if tags::apply_tag_paths(&mut self.shell, &self.db, paths, tag) {
            ctx.request_repaint();
        } else {
            self.notify(ctx, format!("Cannot add the tag {tag}"));
        }
    }

    /// Show the answers of the open, copy and render again threads.
    fn poll_menu_work(&mut self, ctx: &Context) {
        if let Some(result) = self.thumbs.take_regenerate_result() {
            self.notify(ctx, regenerate_text(result));
        }
        self.poll_opens(ctx);
        let Some(receiver) = self.menu.copying.take() else {
            return;
        };
        match receiver.try_recv() {
            Ok(Ok(())) => {}
            Ok(Err(error)) => self.notify(ctx, copy_failure_text(&error)),
            Err(TryRecvError::Empty) => self.menu.copying = Some(receiver),
            Err(TryRecvError::Disconnected) => {
                tracing::warn!("the copy thread ended without an answer");
            }
        }
    }
}

impl App {
    /// Show a notice for each finished open that failed, and keep the opens
    /// that still run.
    fn poll_opens(&mut self, ctx: &Context) {
        let mut failures = Vec::new();
        self.menu
            .opening
            .retain(|receiver| match receiver.try_recv() {
                Ok((report, total)) => {
                    if let Some(text) = report.first_error {
                        failures.push(open_failure_text(&text, report.failed, total));
                    }
                    false
                }
                Err(TryRecvError::Empty) => true,
                Err(TryRecvError::Disconnected) => {
                    tracing::warn!("the open thread ended without an answer");
                    false
                }
            });
        for text in failures {
            self.notify(ctx, text);
        }
    }
}

/// The notice for a failed open of some files. `first` is the first error.
fn open_failure_text(first: &str, failed: usize, total: usize) -> String {
    if total <= 1 {
        first.to_owned()
    } else {
        format!("{first} ({failed} of {total} files failed)")
    }
}

/// The notice for a failed clipboard copy.
fn copy_failure_text(error: &ClipboardError) -> String {
    match error {
        ClipboardError::ToolMissing(package) => format!("Install {package} to copy files"),
        other => format!("Cannot copy files: {other}"),
    }
}

/// The notice for the end of a render again.
fn regenerate_text(result: Result<usize, String>) -> String {
    match result {
        Ok(1) => "Queued 1 thumbnail to render again".to_owned(),
        Ok(count) => format!("Queued {count} thumbnails to render again"),
        Err(error) => format!("Cannot render thumbnails again: {error}"),
    }
}

mod delete;
mod undo;
mod writes;

#[cfg(test)]
#[path = "menu_tests.rs"]
mod tests;
