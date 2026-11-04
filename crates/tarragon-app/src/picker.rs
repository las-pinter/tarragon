//! The native folder dialog, run on its own thread.
//!
//! A dialog blocks until the user answers. The interface thread must not
//! wait, so [`Picker`] runs the dialog on a worker thread and the interface
//! polls for the answer each frame.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;

/// The environment variable that replaces the native dialog with a fixed
/// answer. A native dialog cannot run under a virtual display, so smoke
/// runs set it to a folder path.
const FAKE_PICK_VAR: &str = "TARRAGON_FAKE_PICK";

/// What the user is picking a folder for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Purpose {
    /// A folder to open as a library.
    OpenFolder,
    /// The folder that holds the thumbnail cache.
    CacheDir,
}

/// Something that asks the user for a folder and waits for the answer.
pub trait FolderDialog: Send + 'static {
    /// Shows the dialog, opened at `start` when given. Returns the chosen
    /// folder, or `None` when the user cancels or no dialog can open.
    fn pick(&self, start: Option<&Path>) -> Option<PathBuf>;
}

/// The desktop folder dialog, reached through the desktop portal.
pub struct NativeDialog;

impl FolderDialog for NativeDialog {
    fn pick(&self, start: Option<&Path>) -> Option<PathBuf> {
        let mut dialog = rfd::FileDialog::new();
        if let Some(start) = start {
            dialog = dialog.set_directory(start);
        }
        dialog.pick_folder()
    }
}

/// A dialog that always answers with the same folder.
struct FixedDialog(PathBuf);

impl FolderDialog for FixedDialog {
    fn pick(&self, _start: Option<&Path>) -> Option<PathBuf> {
        Some(self.0.clone())
    }
}

/// The wake-up call that asks the interface to draw a new frame.
type Repaint = Arc<dyn Fn() + Send + Sync>;

/// Runs one folder dialog at a time off the interface thread.
///
/// [`Picker::new`] uses the native dialog. When the `TARRAGON_FAKE_PICK`
/// environment variable holds a path, it uses a dialog that answers that
/// path instead, so smoke runs work without a screen to click on. The
/// variable is read once, when the picker is built, and it works in every
/// build. It only chooses a folder the user could have chosen, and the
/// caller still normalizes whatever path comes back.
pub struct Picker {
    dialog: Arc<Mutex<Box<dyn FolderDialog>>>,
    repaint: Repaint,
    /// The purpose and the answer channel of the open dialog, if any.
    pending: Option<(Purpose, Receiver<Option<PathBuf>>)>,
}

impl Picker {
    /// Builds a picker. `repaint` runs on the worker thread each time a
    /// dialog answers.
    pub fn new(repaint: impl Fn() + Send + Sync + 'static) -> Self {
        Self::from_env_value(std::env::var_os(FAKE_PICK_VAR), repaint)
    }

    /// Builds a picker from the value of the fake-pick variable.
    fn from_env_value(fake: Option<OsString>, repaint: impl Fn() + Send + Sync + 'static) -> Self {
        let dialog: Box<dyn FolderDialog> = match fake {
            Some(path) => Box::new(FixedDialog(PathBuf::from(path))),
            None => Box::new(NativeDialog),
        };
        Self::with_dialog(dialog, repaint)
    }

    /// Builds a picker around any dialog.
    pub fn with_dialog(
        dialog: Box<dyn FolderDialog>,
        repaint: impl Fn() + Send + Sync + 'static,
    ) -> Self {
        Self {
            dialog: Arc::new(Mutex::new(dialog)),
            repaint: Arc::new(repaint),
            pending: None,
        }
    }

    /// Opens a dialog for `purpose` at `start`. Does nothing while an
    /// earlier dialog has no answer yet.
    pub fn ask(&mut self, purpose: Purpose, start: Option<PathBuf>) {
        if self.pending.is_some() {
            return;
        }
        let (tx, rx) = mpsc::channel();
        let dialog = Arc::clone(&self.dialog);
        let repaint = Arc::clone(&self.repaint);
        let spawned = thread::Builder::new()
            .name("tarragon-picker".into())
            .spawn(move || {
                let answer = dialog
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .pick(start.as_deref());
                // The picker may be gone by now; a lost answer is fine.
                let _ = tx.send(answer);
                repaint();
            });
        if let Err(err) = spawned {
            // The closure and its sender are dropped, so `poll` reports a
            // cancelled dialog. The repaint makes the interface poll now.
            tracing::warn!("cannot start the folder dialog thread: {err}");
            (self.repaint)();
        }
        self.pending = Some((purpose, rx));
    }

    /// Returns the answer of the open dialog once, then `None` until the
    /// next `ask`. The path is `None` when the user cancelled or the dialog
    /// could not run.
    pub fn poll(&mut self) -> Option<(Purpose, Option<PathBuf>)> {
        let (purpose, rx) = self.pending.as_ref()?;
        let purpose = *purpose;
        let answer = match rx.try_recv() {
            Ok(answer) => answer,
            Err(TryRecvError::Empty) => return None,
            Err(TryRecvError::Disconnected) => {
                tracing::warn!("the folder dialog ended without an answer");
                None
            }
        };
        self.pending = None;
        Some((purpose, answer))
    }
}

#[cfg(test)]
mod tests;
