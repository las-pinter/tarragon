//! The priority job queue.
//!
//! Two ordered sets hold the jobs: the ones for visible cells and the rest.
//! Both keep scan order. A worker takes from the visible set first. An index
//! from path to sequence number finds a queued job without a walk over all
//! jobs, so a scroll does not hold the lock for the size of the folder.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Condvar, Mutex, MutexGuard, PoisonError};

use super::job::Job;

#[derive(Default)]
struct State {
    visible: BTreeMap<u64, Job>,
    rest: BTreeMap<u64, Job>,
    /// The sequence number of each queued job, in `visible` or `rest`.
    index: HashMap<PathBuf, u64>,
    wanted: HashSet<PathBuf>,
    /// The next free sequence number, so a late job sorts after the rest.
    next_seq: u64,
    shutdown: bool,
    /// The files of the jobs a worker took and has not finished.
    running: Vec<PathBuf>,
    /// The files that [`JobQueue::settle`] waits for. A result of one of them
    /// is stale, so a worker drops it instead of waiting for room.
    settling: HashSet<PathBuf>,
}

impl State {
    /// Queue `job` at the end of the rest, unless its path is queued.
    fn push_rest(&mut self, job: Job) {
        if let Some(seq) = self.claim(&job.path) {
            self.rest.insert(seq, job);
        }
    }

    /// Queue `job` at the end of the visible set, unless its path is queued.
    fn push_visible(&mut self, job: Job) {
        if let Some(seq) = self.claim(&job.path) {
            self.visible.insert(seq, job);
        }
    }

    /// Take the next sequence number for `path` and index it, or none when
    /// the path is queued already.
    fn claim(&mut self, path: &Path) -> Option<u64> {
        if self.index.contains_key(path) {
            return None;
        }
        let seq = self.next_seq;
        self.next_seq += 1;
        self.index.insert(path.to_path_buf(), seq);
        Some(seq)
    }

    /// Drop the queued job of `path`, if there is one.
    fn remove(&mut self, path: &Path) {
        if let Some(seq) = self.index.remove(path) {
            self.visible.remove(&seq);
            self.rest.remove(&seq);
        }
    }
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

    /// Drop every queued job and queue `jobs` in the given order. A second
    /// job of a path is dropped.
    pub(super) fn replace(&self, jobs: Vec<Job>) {
        let mut state = self.lock();
        state.visible.clear();
        state.rest.clear();
        state.index.clear();
        state.wanted.clear();
        state.next_seq = 0;
        for job in jobs {
            state.push_rest(job);
        }
        drop(state);
        self.wake.notify_all();
    }

    /// Put the jobs for `visible` keys first. Other jobs go back to the rest.
    ///
    /// The work is in the size of the visible set and of `visible`, not of
    /// the whole queue.
    pub(super) fn reprioritise(&self, visible: &HashSet<PathBuf>) {
        let mut state = self.lock();
        if state.wanted == *visible {
            return;
        }
        let leaving: Vec<u64> = state
            .visible
            .iter()
            .filter(|(_, job)| !visible.contains(&job.path))
            .map(|(seq, _)| *seq)
            .collect();
        for seq in leaving {
            if let Some(job) = state.visible.remove(&seq) {
                state.rest.insert(seq, job);
            }
        }
        for path in visible {
            let Some(seq) = state.index.get(path).copied() else {
                continue;
            };
            if let Some(job) = state.rest.remove(&seq) {
                state.visible.insert(seq, job);
            }
        }
        state.wanted.clone_from(visible);
    }

    /// Queue `jobs` for the visible set again. A path that is still queued is
    /// not added twice.
    pub(super) fn request(&self, jobs: Vec<Job>) {
        let mut state = self.lock();
        for job in jobs {
            state.push_visible(job);
        }
        drop(state);
        self.wake.notify_all();
    }

    /// Queue `jobs` for the visible set. A job still waiting for the same path
    /// is dropped, so the new one runs instead of it. A running job is not
    /// touched. It returns how many jobs it queued.
    pub(super) fn retry(&self, jobs: Vec<Job>) -> usize {
        let mut state = self.lock();
        let queued = jobs.len();
        for job in jobs {
            state.remove(&job.path);
            state.push_visible(job);
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
                state.index.remove(&job.path);
                // Counted under the same lock as the pop, so no gap shows an
                // empty queue with a job in hand.
                state.running.push(job.path.clone());
                return Some(job);
            }
            state = self
                .wake
                .wait(state)
                .unwrap_or_else(PoisonError::into_inner);
        }
    }

    /// Mark the job of `path`, taken by [`Self::next`], as done.
    pub(super) fn finish(&self, path: &Path) {
        let mut state = self.lock();
        if let Some(index) = state.running.iter().position(|running| running == path) {
            state.running.swap_remove(index);
        }
        drop(state);
        self.idle.notify_all();
    }

    /// Block until no worker holds a job, or until [`Self::shutdown`].
    pub(super) fn wait_idle(&self) {
        let mut state = self.lock();
        while !state.running.is_empty() && !state.shutdown {
            state = self
                .idle
                .wait(state)
                .unwrap_or_else(PoisonError::into_inner);
        }
    }

    /// Drop the waiting jobs of `keys`, then block until no worker renders one
    /// of them, or until [`Self::shutdown`]. Other jobs keep running and
    /// keep their place.
    ///
    /// Nothing queues a job of `keys` while this waits, so the files are
    /// quiet when it returns: no render of them runs or will start.
    pub(super) fn settle(&self, keys: &HashSet<PathBuf>) {
        let mut state = self.lock();
        for key in keys {
            state.remove(key);
        }
        state.settling.clone_from(keys);
        while state.running.iter().any(|path| keys.contains(path)) && !state.shutdown {
            state = self
                .idle
                .wait(state)
                .unwrap_or_else(PoisonError::into_inner);
        }
        state.settling.clear();
    }

    /// True while [`Self::settle`] waits for `path`.
    pub(super) fn is_settling(&self, path: &Path) -> bool {
        self.lock().settling.contains(path)
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
            colors: None,
            needs_colors: false,
        }
    }

    fn names(queue: &JobQueue, count: usize) -> Vec<PathBuf> {
        (0..count)
            .filter_map(|_| queue.next())
            .map(|job| job.path)
            .collect()
    }

    /// Run `settle` on another thread and report whether it returned in time.
    fn settles(queue: &std::sync::Arc<JobQueue>, keys: &[&str]) -> bool {
        let keys: HashSet<PathBuf> = keys.iter().map(PathBuf::from).collect();
        let (tx, rx) = std::sync::mpsc::channel();
        let queue = std::sync::Arc::clone(queue);
        std::thread::spawn(move || {
            queue.settle(&keys);
            let _ = tx.send(());
        });
        rx.recv_timeout(std::time::Duration::from_secs(5)).is_ok()
    }

    #[test]
    fn settle_drops_the_waiting_jobs_of_the_keys_and_keeps_the_order_of_the_rest() {
        let queue = std::sync::Arc::new(JobQueue::default());
        queue.replace(vec![job("a"), job("b"), job("c"), job("d")]);
        queue.request(vec![job("e")]);

        assert!(settles(&queue, &["b", "e"]));

        assert_eq!(queue.queued(), 3);
        let order = names(&queue, 3);
        assert_eq!(
            order,
            [PathBuf::from("a"), PathBuf::from("c"), PathBuf::from("d")]
        );
    }

    #[test]
    fn settle_does_not_wait_for_a_running_job_of_another_key() {
        let queue = std::sync::Arc::new(JobQueue::default());
        queue.replace(vec![job("x")]);
        let taken = queue.next().expect("a job");
        assert_eq!(taken.path, PathBuf::from("x"));

        assert!(settles(&queue, &["y"]), "x is not waited for");
        assert!(!queue.is_settling(Path::new("y")), "the mark is cleared");
        queue.finish(Path::new("x"));
    }

    #[test]
    fn replace_with_nothing_leaves_no_queued_job() {
        let queue = JobQueue::default();
        queue.replace(vec![job("old-rest")]);
        queue.request(vec![job("old-visible")]);
        queue.replace(Vec::new());
        assert_eq!(queue.queued(), 0);
    }

    /// The paths `a`, `b`, ... as a set.
    fn set(names: &[&str]) -> HashSet<PathBuf> {
        names.iter().map(PathBuf::from).collect()
    }

    #[test]
    fn reprioritise_puts_the_visible_jobs_first_and_returns_the_others_to_scan_order() {
        let queue = JobQueue::default();
        queue.replace(["a", "b", "c", "d"].map(job).to_vec());
        queue.reprioritise(&set(&["c"]));
        queue.reprioritise(&set(&["d"]));
        assert_eq!(names(&queue, 4), ["d", "a", "b", "c"].map(PathBuf::from));
    }

    #[test]
    fn request_skips_a_queued_path_and_queues_a_taken_one_again() {
        let queue = JobQueue::default();
        queue.replace(["a", "b"].map(job).to_vec());
        assert_eq!(names(&queue, 1), [PathBuf::from("a")]);
        queue.request(["a", "b"].map(job).to_vec());
        assert_eq!(queue.queued(), 2, "a again, b once");
        // The requested `a` is visible, so it runs before the waiting `b`.
        assert_eq!(names(&queue, 2), ["a", "b"].map(PathBuf::from));
    }

    #[test]
    fn retry_replaces_the_waiting_job_of_the_path() {
        let queue = JobQueue::default();
        queue.replace(["a", "b", "c"].map(job).to_vec());
        assert_eq!(queue.retry(vec![job("c")]), 1);
        assert_eq!(queue.queued(), 3);
        assert_eq!(names(&queue, 3), ["c", "a", "b"].map(PathBuf::from));
    }

    #[test]
    fn replace_drops_a_second_job_of_a_path() {
        let queue = JobQueue::default();
        queue.replace(["a", "b", "a"].map(job).to_vec());
        assert_eq!(queue.queued(), 2);
    }
}
