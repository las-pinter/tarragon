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
    /// The next free sequence number, so a late job sorts after the rest.
    next_seq: u64,
    shutdown: bool,
    /// The jobs a worker took and has not finished.
    busy: usize,
}

/// A blocking queue shared by the workers and the pipeline.
#[derive(Default)]
pub(super) struct JobQueue {
    state: Mutex<State>,
    wake: Condvar,
    /// Signals that a worker finished a job.
    idle: Condvar,
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
        state.next_seq = jobs.len() as u64;
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

    /// Queue `jobs` for the visible set again. A path that is still queued is
    /// not added twice.
    pub(super) fn request(&self, jobs: Vec<Job>) {
        let mut state = self.lock();
        let pending: HashSet<PathBuf> = state
            .visible
            .values()
            .chain(state.rest.values())
            .map(|job| job.path.clone())
            .collect();
        for job in jobs.into_iter().filter(|job| !pending.contains(&job.path)) {
            let seq = state.next_seq;
            state.next_seq += 1;
            state.visible.insert(seq, job);
        }
        drop(state);
        self.wake.notify_all();
    }

    /// Queue `jobs` for the visible set. A job still waiting for the same path
    /// is dropped, so the new one runs instead of it. A running job is not
    /// touched. It returns how many jobs it queued.
    pub(super) fn retry(&self, jobs: Vec<Job>) -> usize {
        let mut state = self.lock();
        let paths: HashSet<PathBuf> = jobs.iter().map(|job| job.path.clone()).collect();
        state.visible.retain(|_, job| !paths.contains(&job.path));
        state.rest.retain(|_, job| !paths.contains(&job.path));
        let queued = jobs.len();
        for job in jobs {
            let seq = state.next_seq;
            state.next_seq += 1;
            state.visible.insert(seq, job);
        }
        drop(state);
        self.wake.notify_all();
        queued
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
                // Counted under the same lock as the pop, so no gap shows an
                // empty queue with a job in hand.
                state.busy += 1;
                return Some(job);
            }
            state = self
                .wake
                .wait(state)
                .unwrap_or_else(PoisonError::into_inner);
        }
    }

    /// Mark one job taken by [`Self::next`] as done.
    pub(super) fn finish(&self) {
        let mut state = self.lock();
        state.busy = state.busy.saturating_sub(1);
        drop(state);
        self.idle.notify_all();
    }

    /// Block until no worker holds a job, or until [`Self::shutdown`].
    pub(super) fn wait_idle(&self) {
        let mut state = self.lock();
        while state.busy > 0 && !state.shutdown {
            state = self
                .idle
                .wait(state)
                .unwrap_or_else(PoisonError::into_inner);
        }
    }

    /// The number of jobs waiting for a worker.
    #[cfg(test)]
    fn queued(&self) -> usize {
        let state = self.lock();
        state.visible.len() + state.rest.len()
    }

    /// True after [`Self::shutdown`].
    pub(super) fn is_shutdown(&self) -> bool {
        self.lock().shutdown
    }

    /// Wake every worker and make them stop.
    pub(super) fn shutdown(&self) {
        self.lock().shutdown = true;
        self.wake.notify_all();
        self.idle.notify_all();
    }
}

#[cfg(test)]
mod tests {
    use tarragon_cache::store::SourceStamp;

    use super::*;

    fn job(name: &str) -> Job {
        Job {
            generation: 1,
            path: PathBuf::from(name),
            stamp: SourceStamp { mtime: 0, size: 0 },
            stored: None,
            folder: String::new(),
            uuid: String::new(),
            known_failure: None,
            retry: false,
            colors: None,
            needs_colors: false,
        }
    }

    #[test]
    fn replace_with_nothing_leaves_no_queued_job() {
        let queue = JobQueue::default();
        queue.replace(vec![job("old-rest")]);
        queue.request(vec![job("old-visible")]);
        queue.replace(Vec::new());
        assert_eq!(queue.queued(), 0);
    }
}
