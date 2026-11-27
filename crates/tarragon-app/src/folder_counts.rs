//! The image counts of the visible sidebar folders.
//!
//! A worker thread counts each folder the sidebar newly shows, and a watcher
//! over the visible folders asks for a recount when one changes. The results
//! wait in a channel until the frame loop hands them to the sidebar. A
//! missing folder gives no count; nothing is ever removed from the disk or
//! the database here.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

use tarragon_core::error::ScanError;
use tarragon_scan::count::count_supported;
use tarragon_scan::watch_set::{Change, FolderSetWatcher};
use tarragon_ui::sidebar::panel::Sidebar;

use crate::pipeline::Repaint;

/// One counted folder: its path and its count, or none when it cannot be listed.
type Counted = (PathBuf, Option<u32>);

/// A message to the count worker.
enum Request {
    /// The tree now shows exactly these folders.
    Show(Vec<PathBuf>),
    /// The watcher saw a change.
    Changed(Change),
    /// The app is closing.
    Stop,
}

/// The handle of the count worker.
///
/// The worker owns the watcher and the visible set, so neither a watch nor a
/// listing ever runs on the frame loop: both can block on a dead mount. The
/// worker is not joined on drop for the same reason; it ends at the stop
/// message once it is free.
pub struct FolderCounts {
    /// Messages to the worker.
    requests: Sender<Request>,
    /// Counted folders, waiting for the frame loop.
    results: Receiver<Counted>,
}

impl FolderCounts {
    /// Start the worker. `repaint` wakes the window after a batch is counted,
    /// because no input event follows a count.
    pub fn start(repaint: Repaint) -> Self {
        let (requests, inbox) = mpsc::channel::<Request>();
        let (outbox, results) = mpsc::channel::<Counted>();
        let for_watcher = requests.clone();
        let spawned = thread::Builder::new()
            .name("tarragon-counts".to_owned())
            .spawn(move || {
                let watcher = start_watcher(for_watcher);
                count_loop(&inbox, &outbox, &repaint, watcher);
            });
        if let Err(error) = spawned {
            tracing::warn!("cannot start the folder count thread: {error}");
        }
        Self { requests, results }
    }

    /// Count and watch exactly `folders`: the worker watches the new set,
    /// then counts the folders it did not show before.
    pub fn set_visible(&self, folders: &[PathBuf]) {
        // A send fault means the worker never started; counts stay empty.
        let _ = self.requests.send(Request::Show(folders.to_vec()));
    }

    /// Hand the counted folders to the sidebar. Returns whether any arrived,
    /// so the caller can repaint.
    pub fn apply(&self, sidebar: &mut Sidebar) -> bool {
        let mut any = false;
        while let Ok((path, count)) = self.results.try_recv() {
            sidebar.set_count(&path, count);
            any = true;
        }
        any
    }
}

impl Drop for FolderCounts {
    fn drop(&mut self) {
        let _ = self.requests.send(Request::Stop);
    }
}

/// Start the watcher; a change goes to the worker. A watcher that cannot
/// start is logged, and the counts then refresh only when a row shows again.
fn start_watcher(requests: Sender<Request>) -> Option<FolderSetWatcher> {
    let on_change = move |change: Change| {
        let _ = requests.send(Request::Changed(change));
    };
    match FolderSetWatcher::start(on_change) {
        Ok(watcher) => Some(watcher),
        Err(error) => {
            tracing::warn!("cannot watch the sidebar folders: {error}");
            None
        }
    }
}

/// Log the folders the watcher refused that were not reported before. The
/// count of such a folder stops refreshing, so the first new error and the
/// number show at the default level; the rest go to debug. A folder stays
/// reported, so a refusal that repeats on every tree change warns once.
fn warn_unwatched(errors: &[ScanError], reported: &mut BTreeSet<PathBuf>) {
    let new = new_failures(errors, reported);
    let Some(first) = new.first() else {
        return;
    };
    tracing::warn!(
        "{} folder counts will not refresh on change; the first: {first}",
        new.len()
    );
    for error in &new[1..] {
        tracing::debug!("a folder count is not watched: {error}");
    }
}

/// The errors whose folder is not in `reported`, which then holds them.
fn new_failures<'a>(
    errors: &'a [ScanError],
    reported: &mut BTreeSet<PathBuf>,
) -> Vec<&'a ScanError> {
    errors
        .iter()
        .filter(|error| match error {
            ScanError::Watch { path, .. } => reported.insert(path.clone()),
            ScanError::ReadFolder { .. } => true,
        })
        .collect()
}

/// Serve requests until the stop message.
///
/// Requests that wait together merge, so a burst counts each folder once. A
/// new visible set is watched before its new folders are counted, so a file
/// added after the listing is still seen. A change counts again only the
/// folders the tree shows.
fn count_loop(
    inbox: &Receiver<Request>,
    outbox: &Sender<Counted>,
    repaint: &Repaint,
    mut watcher: Option<FolderSetWatcher>,
) {
    let mut visible: BTreeSet<PathBuf> = BTreeSet::new();
    let mut reported: BTreeSet<PathBuf> = BTreeSet::new();
    while let Ok(first) = inbox.recv() {
        let mut due = BTreeSet::new();
        for request in std::iter::once(first).chain(inbox.try_iter()) {
            match request {
                Request::Show(folders) => {
                    if let Some(watcher) = &mut watcher {
                        warn_unwatched(&watcher.set_folders(&folders), &mut reported);
                    }
                    let shown: BTreeSet<PathBuf> = folders.into_iter().collect();
                    due.extend(shown.difference(&visible).cloned());
                    visible = shown;
                }
                Request::Changed(Change::All) => due.extend(visible.iter().cloned()),
                Request::Changed(Change::Folders(touched)) => {
                    due.extend(touched.intersection(&visible).cloned());
                }
                Request::Stop => return,
            }
        }
        if due.is_empty() {
            continue;
        }
        for folder in due {
            let count = count_supported(&folder);
            if outbox.send((folder, count)).is_err() {
                return;
            }
        }
        repaint();
    }
}

#[cfg(test)]
mod tests;
