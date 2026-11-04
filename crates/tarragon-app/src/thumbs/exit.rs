//! Stopping the pipeline at exit, and clearing the `full` tier afterwards.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use tarragon_cache::store::CacheStore;

use super::{Slot, Thumbs};

/// Lets `main` wait for the pipeline to finish after the window is gone.
#[derive(Clone, Default)]
pub struct ExitWait(pub(super) Arc<Mutex<Option<JoinHandle<()>>>>);

impl ExitWait {
    /// Block until the pipeline has stopped and stored its rows.
    pub fn wait(&self) {
        let handle = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
        if let Some(handle) = handle {
            let _ = handle.join();
        }
    }
}

/// What the exit clears: the `full` tier of the cache at `cache_root`.
pub struct ExitClear {
    /// The cache folder.
    pub cache_root: PathBuf,
    /// The data folder. The clear refuses to delete it, so the database stays.
    pub data_root: PathBuf,
}

impl ExitClear {
    /// Delete the `full` tier files. A failure is logged and never stops the exit.
    fn run(&self) {
        let cache = CacheStore::new(self.cache_root.clone());
        if let Err(error) = cache.clear_full_res_on_exit(true, Some(&self.data_root)) {
            tracing::warn!("cannot clear the full size cache on exit: {error}");
        }
    }
}

impl Thumbs {
    /// Hand the pipeline to a thread that stops it, and give `wait` the
    /// handle. The drop blocks until renders in flight end, and a PSD render
    /// can take seconds, so the window must not wait for it.
    ///
    /// With `exit`, the same thread then clears the `full` tier. The clear
    /// runs after the drop, when every worker has ended, so no render can
    /// write a file behind it.
    pub fn shutdown(&mut self, wait: &ExitWait, exit: Option<ExitClear>) {
        let slot = std::mem::replace(&mut self.slot, Slot::Gone);
        let spawned = std::thread::Builder::new()
            .name("tarragon-shutdown".to_owned())
            .spawn(move || {
                match slot {
                    Slot::Idle(pipeline) => drop(pipeline),
                    // The open thread returns the pipeline; dropping it ends the work.
                    Slot::Opening { done, .. } => drop(done.recv()),
                    Slot::Working { done, .. } => drop(done.recv()),
                    Slot::Gone => {}
                }
                if let Some(exit) = exit {
                    exit.run();
                }
            });
        match spawned {
            Ok(handle) => {
                *wait
                    .0
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(handle);
            }
            Err(error) => tracing::warn!("cannot stop the pipeline in the background: {error}"),
        }
    }
}
