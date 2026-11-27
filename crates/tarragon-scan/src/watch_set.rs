//! A watcher over a changing set of folders, for the sidebar counts.
//!
//! One backend holds a non-recursive watch on each folder of the set. A burst
//! of events gives one callback after [`DEBOUNCE`], naming the folders the
//! events touched. A backend error can mean lost events, so it reports
//! [`Change::All`]. Dropping the watcher drops the backend and joins the
//! debounce thread, so no callback runs after the drop.

use std::collections::BTreeSet;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use tarragon_core::error::ScanError;

use crate::watch::{DEBOUNCE, MAX_WAIT, is_change};

/// What changed in one quiet window.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Change {
    /// The events named these paths and their parent folders. A caller keeps
    /// the ones it shows; the others are files or folders it does not track.
    Folders(BTreeSet<PathBuf>),
    /// The backend reported an error, so any watched folder may have changed.
    All,
}

/// One message from the backend to the debounce thread.
enum Message {
    /// A change event with its paths.
    Paths(Vec<PathBuf>),
    /// A backend error.
    Lost,
}

/// A running watcher over a set of folders.
pub struct FolderSetWatcher {
    backend: Option<RecommendedWatcher>,
    watched: BTreeSet<PathBuf>,
    debounce: Option<JoinHandle<()>>,
}

impl FolderSetWatcher {
    /// Start a watcher with no folders yet.
    ///
    /// `on_change` runs on the debounce thread after each quiet window. It
    /// must not block for long, because it holds up the next window.
    ///
    /// # Errors
    ///
    /// Returns [`ScanError::Watch`] when the backend or the debounce thread
    /// cannot start.
    pub fn start<F>(on_change: F) -> Result<Self, ScanError>
    where
        F: FnMut(Change) + Send + 'static,
    {
        let (sender, receiver) = mpsc::channel::<Message>();
        let backend = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
            let message = match event {
                Ok(event) if is_change(&event) => Message::Paths(event.paths),
                Ok(_) => return,
                Err(_) => Message::Lost,
            };
            // A send fault means the debounce thread already stopped.
            let _ = sender.send(message);
        })
        .map_err(|error| watch_error(Path::new(""), &error))?;
        let debounce = thread::Builder::new()
            .name("tarragon-watch-set".to_owned())
            .spawn(move || debounce_loop(&receiver, on_change, DEBOUNCE, MAX_WAIT))
            .map_err(|error| watch_error(Path::new(""), &error))?;
        Ok(Self {
            backend: Some(backend),
            watched: BTreeSet::new(),
            debounce: Some(debounce),
        })
    }

    /// Watch exactly `folders`: start the new ones and end the ones left out.
    ///
    /// A folder the backend cannot watch, such as a missing one, stays out of
    /// the set and its error is returned; the others still change.
    pub fn set_folders(&mut self, folders: &[PathBuf]) -> Vec<ScanError> {
        let Some(backend) = self.backend.as_mut() else {
            return Vec::new();
        };
        let wanted: BTreeSet<PathBuf> = folders.iter().cloned().collect();
        for gone in self.watched.difference(&wanted) {
            // The folder may be gone already, which ends its watch anyway.
            let _ = backend.unwatch(gone);
        }
        let mut errors = Vec::new();
        let mut watched = BTreeSet::new();
        for folder in wanted {
            if self.watched.contains(&folder) {
                watched.insert(folder);
                continue;
            }
            match backend.watch(&folder, RecursiveMode::NonRecursive) {
                Ok(()) => {
                    watched.insert(folder);
                }
                Err(error) => errors.push(watch_error(&folder, &error)),
            }
        }
        self.watched = watched;
        errors
    }

    /// The folders watched now.
    #[must_use]
    pub fn folders(&self) -> &BTreeSet<PathBuf> {
        &self.watched
    }

    /// Stop the watch. Safe to call more than once.
    pub fn stop(&mut self) {
        self.backend = None;
        self.watched.clear();
        if let Some(handle) = self.debounce.take() {
            // The loop catches a panicking callback, so the join result holds
            // nothing the caller needs.
            let _ = handle.join();
        }
    }
}

impl Drop for FolderSetWatcher {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Wrap a backend or thread fault as a scan error.
fn watch_error(folder: &Path, reason: impl std::fmt::Display) -> ScanError {
    ScanError::Watch {
        path: folder.to_path_buf(),
        reason: reason.to_string(),
    }
}

/// Add each path of an event and its parent folder to `into`.
///
/// A file event names the file, so its folder is the parent. An event on a
/// watched folder itself, such as its removal, names the folder.
fn collect(paths: Vec<PathBuf>, into: &mut BTreeSet<PathBuf>) {
    for path in paths {
        if let Some(parent) = path.parent() {
            into.insert(parent.to_path_buf());
        }
        into.insert(path);
    }
}

/// Coalesce messages and call `on_change` once per quiet window.
///
/// A burst ends after `quiet` with no message, or after `max_wait` from its
/// first message. A panic in `on_change` is caught. A disconnected channel
/// means the backend dropped, so the thread returns without a call.
fn debounce_loop<F: FnMut(Change)>(
    receiver: &Receiver<Message>,
    mut on_change: F,
    quiet: Duration,
    max_wait: Duration,
) {
    loop {
        let Ok(first) = receiver.recv() else {
            return;
        };
        let mut folders = BTreeSet::new();
        let mut lost = false;
        let mut take = |message: Message| match message {
            Message::Paths(paths) => collect(paths, &mut folders),
            Message::Lost => lost = true,
        };
        take(first);
        let started = Instant::now();
        while let Some(left) = max_wait.checked_sub(started.elapsed()) {
            match receiver.recv_timeout(quiet.min(left)) {
                Ok(message) => take(message),
                Err(RecvTimeoutError::Timeout) => break,
                Err(RecvTimeoutError::Disconnected) => return,
            }
        }
        let change = if lost {
            Change::All
        } else {
            Change::Folders(folders)
        };
        let _ = catch_unwind(AssertUnwindSafe(|| on_change(change)));
    }
}

#[cfg(test)]
mod tests;
