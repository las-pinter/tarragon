//! The choices of the grid context menu that the app carries out.
//!
//! The gallery only raises the events. Work that can stall runs off the UI
//! thread: the clipboard copy on a short-lived thread, and the render again of
//! thumbnails on the work thread. Starting a program does not wait for it.

use std::path::{Path, PathBuf};

use crossbeam_channel::{Receiver, TryRecvError};
use eframe::egui::Context;
use tarragon_ui::gallery::GridMenuEvent;

use super::App;
use crate::clipboard::{self, ClipboardError};
use crate::{editor, tags};

/// What a clipboard copy on its thread answers.
type CopyResult = Result<(), ClipboardError>;

/// What the menu handlers keep between frames.
#[derive(Default)]
pub(super) struct MenuState {
    /// The answer of the running clipboard copy, until it arrives.
    copying: Option<Receiver<CopyResult>>,
}

impl App {
    /// Take what the menu work has finished, then carry out the menu choices
    /// the gallery raised this frame. Runs once per frame, after the draw.
    pub(super) fn apply_menu_events(&mut self, ctx: &Context) {
        self.poll_menu_work(ctx);
        for event in self.shell.gallery_mut().take_menu_events() {
            self.apply_menu_event(ctx, event);
        }
    }

    /// Carry out one menu choice.
    fn apply_menu_event(&mut self, ctx: &Context, event: GridMenuEvent) {
        match event {
            GridMenuEvent::Open(paths) => self.open_in_editor(ctx, &paths),
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
            GridMenuEvent::RenameTo { path, new_stem } => rename_to(&path, &new_stem),
            GridMenuEvent::Duplicate(paths) => duplicate(&paths),
            GridMenuEvent::Paste => paste(),
            GridMenuEvent::ConfirmedDelete { paths, permanent } => {
                confirmed_delete(&paths, permanent);
            }
        }
    }

    /// Show a notice and ask for a frame, because the notice line was drawn
    /// before this call and egui repaints only on input.
    fn notify(&mut self, ctx: &Context, text: String) {
        self.shell.gallery_mut().notify(text);
        ctx.request_repaint();
    }

    /// Start the editor of each file. No call waits for an editor, so many
    /// files cost one process start each. A failure shows one notice.
    fn open_in_editor(&mut self, ctx: &Context, paths: &[PathBuf]) {
        let mut first = None;
        let mut failed = 0;
        for path in paths {
            if let Err(error) = editor::open(&self.db, path) {
                tracing::warn!("cannot open the file in an editor: {error}");
                failed += 1;
                first.get_or_insert_with(|| error.to_string());
            }
        }
        if let Some(text) = first {
            self.notify(ctx, open_failure_text(&text, failed, paths.len()));
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

    /// Show the answers of the copy thread and of the render again.
    fn poll_menu_work(&mut self, ctx: &Context) {
        if let Some(result) = self.thumbs.take_regenerate_result() {
            self.notify(ctx, regenerate_text(result));
        }
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

/// Rename the file to `new_stem`, keeping its extension. Not wired yet: the
/// event is taken and nothing happens.
fn rename_to(_path: &Path, _new_stem: &str) {}

/// Copy each file next to itself. Not wired yet: the event is taken and nothing
/// happens.
fn duplicate(_paths: &[PathBuf]) {}

/// Paste the clipboard files into the open folder. Not wired yet: the event is
/// taken and nothing happens.
fn paste() {}

/// Delete the confirmed files. Not wired yet: the event is taken and nothing
/// happens.
fn confirmed_delete(_paths: &[PathBuf], _permanent: bool) {}

#[cfg(test)]
#[path = "menu_tests.rs"]
mod tests;
