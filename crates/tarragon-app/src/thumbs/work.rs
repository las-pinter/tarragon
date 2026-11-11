//! Cache upkeep that runs on a work thread: a purge, an eviction, or a render
//! again of chosen files.
//!
//! A purge and a render again wait for the renders in flight and an eviction
//! walks the cache, so none may run on the UI thread. The pipeline moves into
//! the thread the same way it does for an open.

use std::collections::HashSet;
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
    /// Delete the cache files of chosen sources and render them again.
    Regenerate,
}

/// What a finished piece of work did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkOutcome {
    /// The files a purge removed.
    Purged(PurgeOutcome),
    /// The rows and bytes an eviction left behind.
    Evicted(Evicted),
    /// How many files were queued to render again.
    Regenerated(usize),
}

/// What the work thread hands back, with the pipeline it borrowed.
pub(super) struct WorkDone {
    pipeline: Pipeline,
    result: Result<WorkOutcome, String>,
}

/// A request that waits for an idle pipeline.
pub(super) enum Queued {
    /// Purge the cache. `data_root` is the directory the purge must never delete.
    Purge { data_root: PathBuf },
    /// Evict down to `cap_gb` GiB.
    Evict { cap_gb: i64 },
    /// Render these files again.
    Regenerate { keys: HashSet<PathBuf> },
}

impl Queued {
    /// The kind of work this request is.
    fn work(&self) -> Work {
        match self {
            Self::Purge { .. } => Work::Purge,
            Self::Evict { .. } => Work::Evict,
            Self::Regenerate { .. } => Work::Regenerate,
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
            Self::Regenerate { keys } => Ok(WorkOutcome::Regenerated(pipeline.regenerate(keys))),
        }
    }
}

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

    /// Ask to render `keys` again. It waits for an idle pipeline like
    /// [`Self::purge`] and runs on the work thread, because it waits for the
    /// renders of those files that already run. Returns false, and queues
    /// nothing, when another work request waits, so it cannot replace one.
    pub fn regenerate(&mut self, keys: HashSet<PathBuf>) -> bool {
        if self.queued.is_some() {
            return false;
        }
        self.queued = Some(Queued::Regenerate { keys });
        (self.repaint)();
        true
    }

    /// The work that runs now, for the busy strip.
    #[must_use]
    pub fn is_working(&self) -> Option<Work> {
        match &self.slot {
            Slot::Working { work, .. } => Some(*work),
            _ => None,
        }
    }

    /// Return the result of the last finished work, once, whatever it was. The
    /// app takes results by kind, so only tests use this.
    #[cfg(test)]
    pub fn take_work_result(&mut self) -> Option<(Work, Result<WorkOutcome, String>)> {
        self.finished.take()
    }

    /// Return the result of a finished [`Self::regenerate`], once. The result
    /// of other work stays for its own taker.
    pub fn take_regenerate_result(&mut self) -> Option<Result<usize, String>> {
        let result = self.take_result_of(Work::Regenerate)?;
        Some(result.map(|outcome| match outcome {
            WorkOutcome::Regenerated(count) => count,
            _ => 0,
        }))
    }

    /// Return the result of a finished purge, once. The result of other work
    /// stays for its own taker.
    pub fn take_purge_result(&mut self) -> Option<Result<WorkOutcome, String>> {
        self.take_result_of(Work::Purge)
    }

    /// Return the result of a finished eviction, once. Its failure is already
    /// logged, so a caller only takes it to clear it.
    pub fn take_evict_result(&mut self) -> Option<Result<WorkOutcome, String>> {
        self.take_result_of(Work::Evict)
    }

    /// Take the last result only when it came from `work`.
    fn take_result_of(&mut self, work: Work) -> Option<Result<WorkOutcome, String>> {
        if !matches!(&self.finished, Some((done, _)) if *done == work) {
            return None;
        }
        self.finished.take().map(|(_, result)| result)
    }
}

impl Thumbs {
    /// True when a purge or an eviction waits for an idle pipeline.
    #[cfg(test)]
    #[must_use]
    pub(crate) fn has_queued_work(&self) -> bool {
        self.queued.is_some()
    }

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
