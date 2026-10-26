//! The priority job queue.
//!
//! Two ordered sets hold the jobs: the ones for visible cells and the rest.
//! Both keep scan order. A worker takes from the visible set first.

use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;
use std::sync::{Condvar, Mutex, MutexGuard, PoisonError};

use super::job::Job;

#[derive(Default)]
struct State {
    visible: BTreeMap<u64, Job>,
    rest: BTreeMap<u64, Job>,
    wanted: HashSet<PathBuf>,
    shutdown: bool,
}

/// A blocking queue shared by the workers and the pipeline.
#[derive(Default)]
pub(super) struct JobQueue {
    state: Mutex<State>,
    wake: Condvar,
}

impl JobQueue {
    fn lock(&self) -> MutexGuard<'_, State> {
        // A panic never leaves the maps half edited, so a poisoned lock is safe.
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Drop every queued job and queue `jobs` in the given order.
    pub(super) fn replace(&self, jobs: Vec<Job>) {
        let mut state = self.lock();
        state.visible.clear();
        state.wanted.clear();
        state.rest = (0..).zip(jobs).collect();
        drop(state);
        self.wake.notify_all();
    }

    /// Put the jobs for `visible` keys first. Other jobs go back to the rest.
    pub(super) fn reprioritise(&self, visible: &HashSet<PathBuf>) {
        let mut state = self.lock();
        if state.wanted == *visible {
            return;
        }
        let mut all = std::mem::take(&mut state.visible);
        all.append(&mut state.rest);
        for (seq, job) in all {
            if visible.contains(&job.path) {
                state.visible.insert(seq, job);
            } else {
                state.rest.insert(seq, job);
            }
        }
        state.wanted.clone_from(visible);
    }

    /// Wait for the next job. It returns `None` after [`Self::shutdown`].
    pub(super) fn next(&self) -> Option<Job> {
        let mut state = self.lock();
        loop {
            if state.shutdown {
                return None;
            }
            if let Some(job) = state
                .visible
                .pop_first()
                .or_else(|| state.rest.pop_first())
                .map(|(_, job)| job)
            {
                return Some(job);
            }
            state = self
                .wake
                .wait(state)
                .unwrap_or_else(PoisonError::into_inner);
        }
    }

    /// True after [`Self::shutdown`].
    pub(super) fn is_shutdown(&self) -> bool {
        self.lock().shutdown
    }

    /// Wake every worker and make them stop.
    pub(super) fn shutdown(&self) {
        self.lock().shutdown = true;
        self.wake.notify_all();
    }
}
