//! The names of the default apps, for the Preview's open button.
//!
//! A lookup asks `xdg-mime` and reads a desktop entry, which can take a moment
//! or hang, so a worker thread runs it. Names are kept per lower-case
//! extension. After one query passes its deadline no more lookups run, so a
//! broken tool costs one deadline at most.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

use crate::editor::choices::default_app_name;
use crate::pipeline::Repaint;

/// One answer of the worker: the extension and the name, or `Err` after a
/// timeout.
type Answer = (String, Result<Option<String>, ()>);

/// The lookup worker and what it has found.
pub struct AppNames {
    /// Files to look up, one per extension.
    requests: Sender<(String, PathBuf)>,
    /// Answers waiting for the frame loop.
    answers: Receiver<Answer>,
    /// The found names. `None` means the lookup gave no name.
    names: HashMap<String, Option<String>>,
    /// Extensions asked for and not answered yet.
    pending: HashSet<String>,
    /// Set after a timeout: no more lookups are sent.
    stopped: bool,
}

impl AppNames {
    /// Start the worker. `repaint` wakes the window when a name arrives.
    pub fn start(repaint: Repaint) -> Self {
        Self::with_lookup(repaint, default_app_name)
    }

    /// Start the worker with its own `lookup`, so tests need no desktop.
    pub(crate) fn with_lookup<E: Send + 'static>(
        repaint: Repaint,
        lookup: impl Fn(&Path) -> Result<Option<String>, E> + Send + 'static,
    ) -> Self {
        let (requests, inbox) = mpsc::channel::<(String, PathBuf)>();
        let (outbox, answers) = mpsc::channel::<Answer>();
        let spawned = thread::Builder::new()
            .name("tarragon-app-names".to_owned())
            .spawn(move || {
                for (key, file) in inbox {
                    let answer = lookup(&file).map_err(|_| ());
                    let timed_out = answer.is_err();
                    if outbox.send((key, answer)).is_err() {
                        return;
                    }
                    repaint();
                    if timed_out {
                        return;
                    }
                }
            });
        if let Err(error) = spawned {
            tracing::warn!("cannot start the app name thread: {error}");
        }
        Self {
            requests,
            answers,
            names: HashMap::new(),
            pending: HashSet::new(),
            stopped: false,
        }
    }

    /// The default app name for files like `file`, when known. An unknown
    /// extension is sent to the worker once.
    pub fn name_for(&mut self, file: &Path) -> Option<&str> {
        let key = key_of(file);
        if !self.names.contains_key(&key) && !self.stopped && self.pending.insert(key.clone()) {
            // A send fault means the worker is gone; the name stays unknown.
            let _ = self.requests.send((key.clone(), file.to_path_buf()));
        }
        self.names.get(&key).and_then(Option::as_deref)
    }

    /// Take the answers that arrived. Returns whether any did, so the caller
    /// can refresh the button.
    pub fn poll(&mut self) -> bool {
        let mut any = false;
        while let Ok((key, answer)) = self.answers.try_recv() {
            self.pending.remove(&key);
            if let Ok(name) = answer {
                self.names.insert(key, name);
            } else {
                tracing::warn!("the default app lookup timed out; app names stay off");
                self.stopped = true;
            }
            any = true;
        }
        any
    }
}

/// The cache key of `file`: its lower-case extension, empty when it has none.
fn key_of(file: &Path) -> String {
    file.extension()
        .map(|ext| ext.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests;
