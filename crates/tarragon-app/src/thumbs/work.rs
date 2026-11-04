//! Cache upkeep that runs on a work thread: a purge or an eviction.
//!
//! A purge waits for the renders in flight and an eviction walks the cache,
//! so neither may run on the UI thread. The pipeline moves into the thread
//! the same way it does for an open.

use std::path::PathBuf;
use std::sync::Arc;

use crossbeam_channel::TryRecvError;
use tarragon_cache::purge::PurgeOutcome;
use tarragon_cache::store::CacheStore;

use super::{Library, Slot, Thumbs};
use crate::maintenance::{self, Evicted};
use crate::pipeline::Pipeline;

/// The kinds of work.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Work {
    /// Delete every cache file and thumbnail row.
    Purge,
    /// Evict the least recently used sources down to the size cap.
    Evict,
}

/// What a finished piece of work did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkOutcome {
    /// The files a purge removed.
    Purged(PurgeOutcome),
    /// The rows and bytes an eviction left behind.
    Evicted(Evicted),
}

/// What the work thread hands back, with the pipeline it borrowed.
pub(super) struct WorkDone {
    pipeline: Pipeline,
    result: Result<WorkOutcome, String>,
}

/// A request that waits for an idle pipeline.
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "only tests queue it until the app does")
)]
pub(super) enum Queued {
    /// Purge the cache. `data_root` is the directory the purge must never delete.
    Purge { data_root: PathBuf },
    /// Evict down to `cap_gb` GiB.
    Evict { cap_gb: i64 },
}

impl Queued {
    /// The kind of work this request is.
    fn work(&self) -> Work {
        match self {
            Self::Purge { .. } => Work::Purge,
            Self::Evict { .. } => Work::Evict,
        }
    }

    /// Run the request. Errors are turned into text for the caller to show,
    /// and are logged here too, because nobody has to take the answer.
    pub(super) fn run(&self, pipeline: &mut Pipeline) -> Result<WorkOutcome, String> {
        let result = self.run_inner(pipeline);
        if let Err(message) = &result {
            tracing::warn!("{:?} failed: {message}", self.work());
        }
        result
    }

    /// Run the request without logging.
    fn run_inner(&self, pipeline: &mut Pipeline) -> Result<WorkOutcome, String> {
        match self {
            Self::Purge { data_root } => pipeline
                .purge(data_root)
                .map(WorkOutcome::Purged)
                .map_err(|error| error.to_string()),
            Self::Evict { cap_gb } => {
                let cache = CacheStore::new(pipeline.cache_root().to_path_buf());
                maintenance::evict(pipeline.database(), &cache, *cap_gb)
                    .map(WorkOutcome::Evicted)
                    .map_err(|error| error.to_string())
            }
        }
    }
}

#[cfg_attr(
    not(test),
    expect(dead_code, reason = "only tests call these until the app does")
)]
impl Thumbs {
    /// Ask for a purge of the cache. It waits for an idle pipeline and runs
    /// before any waiting folder open. A newer work request replaces a waiting
    /// one. `data_root` is the directory the purge refuses to delete.
    pub fn purge(&mut self, data_root: PathBuf) {
        self.queued = Some(Queued::Purge { data_root });
        (self.repaint)();
    }

    /// Ask for an eviction down to `cap_gb` GiB. It waits like [`Self::purge`].
    pub fn evict(&mut self, cap_gb: i64) {
        self.queued = Some(Queued::Evict { cap_gb });
        (self.repaint)();
    }

    /// The work that runs now, for the busy strip.
    #[must_use]
    pub fn is_working(&self) -> Option<Work> {
        match &self.slot {
            Slot::Working { work, .. } => Some(*work),
            _ => None,
        }
    }

    /// Return the result of the last finished work, once.
    pub fn take_work_result(&mut self) -> Option<(Work, Result<WorkOutcome, String>)> {
        self.finished.take()
    }
}

impl Thumbs {
    /// Move the pipeline into a thread that runs the waiting request.
    pub(super) fn start_work(&mut self) {
        if !matches!(self.slot, Slot::Idle(_)) {
            return;
        }
        let Some(queued) = self.queued.take() else {
            return;
        };
        let Slot::Idle(mut pipeline) = std::mem::replace(&mut self.slot, Slot::Gone) else {
            return;
        };
        let work = queued.work();
        let (sender, done) = crossbeam_channel::bounded(1);
        let repaint = Arc::clone(&self.repaint);
        let spawned = std::thread::Builder::new()
            .name("tarragon-work".to_owned())
            .spawn(move || {
                let result = queued.run(&mut pipeline);
                // A closed window drops the answer, and the pipeline with it.
                let _ = sender.send(WorkDone { pipeline, result });
                repaint();
            });
        match spawned {
            Ok(_) => self.slot = Slot::Working { work, done },
            Err(error) => {
                tracing::error!("cannot start the work thread: {error}");
                self.finished = Some((work, Err(error.to_string())));
            }
        }
    }

    /// Take the answer of the work thread, when it is ready.
    pub(super) fn finish_work(&mut self) {
        let Slot::Working { work, done } = &self.slot else {
            return;
        };
        let work = *work;
        let answer = match done.try_recv() {
            Ok(answer) => answer,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => {
                tracing::error!("the work thread ended without an answer");
                self.slot = Slot::Gone;
                let reason = "the work thread ended without an answer".to_owned();
                self.finished = Some((work, Err(reason)));
                return;
            }
        };
        self.slot = Slot::Idle(answer.pipeline);
        if work == Work::Purge {
            self.reopen_after_purge();
        }
        self.finished = Some((work, answer.result));
    }

    /// A purge moved the pipeline generation and deleted every cache file, so
    /// no tile will ever render for the shown tiles. Open the same view again.
    fn reopen_after_purge(&mut self) {
        self.feed.reset(0, []);
        self.synced = None;
        if self.library == Library::Loaded {
            // The library view comes back by itself; the folder stays as it was.
            self.library = Library::Wanted;
            return;
        }
        // A folder asked for during the purge is newer and keeps its place.
        let shown = self.current.take();
        if self.pending.is_none() {
            self.pending = shown;
        }
    }
}
