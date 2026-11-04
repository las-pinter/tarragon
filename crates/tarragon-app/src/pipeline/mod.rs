//! The render pipeline: scan a folder, serve thumbnails from the cache, and
//! render the rest on a small thread pool.
//!
//! `open_folder` scans, stores the scan rows, and queues one job per file.
//! `open_library` queues one job per stored row, with the stamp from the row.
//! Workers read the cache or render, and send results on a channel. Workers
//! never touch the database: one writer thread stores the rows they make.
//! Each open (folder or library) bumps a generation. A job from an older generation is
//! skipped when a worker takes it, and the consumer drops an older result.
//! A render that is already running is not stopped. `drain` drops the queued
//! jobs and waits for the running ones to end.

mod job;
mod library;
mod prune;
mod purge;
mod queue;
mod worker;
mod writer;

#[cfg(test)]
mod tests;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
#[cfg(test)]
use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};

use crossbeam_channel::{Receiver, Sender};
use image::DynamicImage;
use tarragon_cache::store::{CacheStore, SourceStamp};
use tarragon_core::entity::FileInfo;
use tarragon_core::error::{RenderError, ScanError};
use tarragon_db::{Database, DbError};
use tarragon_render::color::extract_dominant_colors;
use tarragon_scan::scan::{build_pool, scan_folder, worker_pool_size};

pub use job::{ErrorKind, ThumbError, ThumbResult};

use job::{ColorState, Job};
use queue::JobQueue;
use worker::{ColorFn, Shared};

/// The results channel holds this many results. It is small so a UI that
/// stops reading holds back the workers instead of piling up decoded images
/// in memory; 64 thumbs of 256 px are about 16 MiB.
const RESULT_BOUND: usize = 64;

/// A render function. Tests swap it to count renders or to block.
pub type RenderFn = Arc<dyn Fn(&Path) -> Result<DynamicImage, RenderError> + Send + Sync>;

/// A callback that asks the UI to repaint. A worker calls it after each send.
pub type Repaint = Arc<dyn Fn() + Send + Sync>;

/// Failures seen in this run, by path. A failure stays until the source stamp
/// changes. The database has no column for it, so a restart tries again.
type Failures = Arc<Mutex<HashMap<PathBuf, (SourceStamp, ThumbError)>>>;

/// How often each file was asked for again since the last open. A worker
/// notes the count when it takes a job. If the count moved by the time the
/// render ends, a newer render exists, so the older result is dropped. Lock it
/// only while holding the failures lock, except for a plain read.
type Retried = Arc<Mutex<HashMap<PathBuf, u64>>>;

/// Why the pipeline cannot start or open a folder.
#[derive(Debug, thiserror::Error)]
pub enum PipelineError {
    /// The folder scan failed.
    #[error(transparent)]
    Scan(#[from] ScanError),
    /// The scan thread pool did not start.
    #[error("cannot start the scan pool: {0}")]
    ScanPool(String),
    /// The path is not a folder. `open_folder` itself reads such a path as
    /// empty; the app checks first.
    #[error("not a folder")]
    NotAFolder,
    /// A database call failed.
    #[error(transparent)]
    Db(#[from] DbError),
    /// A thread did not start.
    #[error("cannot start a pipeline thread: {0}")]
    Spawn(#[from] std::io::Error),
    /// A cache call failed, for example a purge of an unsafe root.
    #[error(transparent)]
    Cache(#[from] tarragon_cache::CacheError),
}

/// What the pipeline needs to start.
pub struct Config {
    /// The cache that holds the tier files.
    pub cache: CacheStore,
    /// The number of worker threads.
    pub workers: usize,
    /// The render function.
    pub render: RenderFn,
}

impl Config {
    /// The production setup: the registry renderer and the scan pool size.
    #[must_use]
    pub fn new(cache: CacheStore) -> Self {
        Self {
            cache,
            workers: worker_pool_size(),
            render: Arc::new(tarragon_render::registry::render),
        }
    }
}

/// What `open_folder` found.
#[derive(Debug)]
pub struct Opened {
    /// The generation that tags every result of this folder.
    pub generation: u64,
    /// The scanned files, so the caller can show a tile for each at once.
    pub files: Vec<FileInfo>,
}

/// The receiving end for the UI. It never returns a stale result.
pub struct Results {
    receiver: Receiver<ThumbResult>,
    generation: Arc<AtomicU64>,
    store_epoch: Arc<AtomicU64>,
}

impl Results {
    /// A count that goes up after each writer pass that stored data. A change
    /// means new rows or colour tags are committed.
    #[must_use]
    pub fn store_epoch(&self) -> u64 {
        self.store_epoch.load(Ordering::Acquire)
    }

    /// The next current result, if one is ready. Stale results are dropped.
    #[must_use]
    pub fn try_recv(&self) -> Option<ThumbResult> {
        while let Ok(result) = self.receiver.try_recv() {
            if self.is_current(&result) {
                return Some(result);
            }
        }
        None
    }

    /// Wait up to `timeout` for the next current result.
    #[cfg(test)]
    #[must_use]
    pub fn recv_timeout(&self, timeout: Duration) -> Option<ThumbResult> {
        let end = std::time::Instant::now() + timeout;
        loop {
            let left = end.saturating_duration_since(std::time::Instant::now());
            let result = self.receiver.recv_timeout(left).ok()?;
            if self.is_current(&result) {
                return Some(result);
            }
        }
    }

    /// Up to `max` current results that are ready now.
    #[must_use]
    pub fn drain(&self, max: usize) -> Vec<ThumbResult> {
        std::iter::from_fn(|| self.try_recv()).take(max).collect()
    }

    fn is_current(&self, result: &ThumbResult) -> bool {
        result.generation == self.generation.load(Ordering::Acquire)
    }
}

/// The render pipeline. Dropping it stops the threads and stores queued rows.
///
/// `Drop` blocks until every render in flight has finished, because a render
/// is not stopped part way. An owner that cannot wait, such as the UI thread,
/// should drop the pipeline on another thread.
pub struct Pipeline {
    db: Arc<Database>,
    /// The cache root. A cache file is removed only inside it.
    cache_root: PathBuf,
    queue: Arc<JobQueue>,
    generation: Arc<AtomicU64>,
    failures: Failures,
    retried: Retried,
    /// The jobs of the last open (folder or library), kept so a dropped texture can be asked for again.
    jobs: HashMap<PathBuf, Job>,
    rows: Option<Sender<writer::Message>>,
    workers: Vec<JoinHandle<()>>,
    writer: Option<JoinHandle<()>>,
}

impl Pipeline {
    /// Start the worker threads and the writer thread.
    ///
    /// # Errors
    ///
    /// Returns [`PipelineError::Spawn`] when a thread cannot start.
    pub fn new(
        config: Config,
        db: Arc<Database>,
        repaint: Repaint,
    ) -> Result<(Self, Results), PipelineError> {
        Self::with_extract(config, db, repaint, extract_dominant_colors)
    }

    /// Start the threads with a given colour extraction function.
    fn with_extract(
        config: Config,
        db: Arc<Database>,
        repaint: Repaint,
        extract: ColorFn,
    ) -> Result<(Self, Results), PipelineError> {
        let queue = Arc::new(JobQueue::default());
        let generation = Arc::new(AtomicU64::new(0));
        let failures = Failures::default();
        let retried = Retried::default();
        let (result_tx, result_rx) = crossbeam_channel::bounded(RESULT_BOUND);
        let (row_tx, row_rx) = crossbeam_channel::unbounded();
        let store_epoch = Arc::new(AtomicU64::new(0));
        let writer_epoch = Arc::clone(&store_epoch);
        let writer_repaint = Arc::clone(&repaint);

        let mut pipeline = Self {
            db: Arc::clone(&db),
            cache_root: config.cache.root().to_path_buf(),
            queue: Arc::clone(&queue),
            generation: Arc::clone(&generation),
            failures: Arc::clone(&failures),
            retried: Arc::clone(&retried),
            jobs: HashMap::new(),
            rows: Some(row_tx.clone()),
            workers: Vec::new(),
            writer: None,
        };
        // A failed spawn drops `pipeline`, which stops the threads so far.
        pipeline.writer = Some(
            std::thread::Builder::new()
                .name("tarragon-writer".to_owned())
                .spawn(move || writer::run(&db, &row_rx, &writer_epoch, &writer_repaint))?,
        );
        let shared = Arc::new(Shared {
            queue,
            generation: Arc::clone(&generation),
            cache: config.cache,
            render: config.render,
            failures,
            retried,
            results: result_tx,
            rows: row_tx,
            repaint,
            extract,
        });
        for index in 0..config.workers.max(1) {
            let shared = Arc::clone(&shared);
            pipeline.workers.push(
                std::thread::Builder::new()
                    .name(format!("tarragon-render-{index}"))
                    .spawn(move || worker::run(&shared))?,
            );
        }
        let results = Results {
            receiver: result_rx,
            generation,
            store_epoch,
        };
        Ok((pipeline, results))
    }

    /// Scan `folder`, store the scan rows, and queue a job for each file.
    /// It returns the new generation and the files. Jobs of the old folder are
    /// dropped.
    ///
    /// The scan and the row write run on the calling thread. Stored rows and
    /// cache files of direct-child files that are no longer in the folder are
    /// deleted; their tags stay. A scan that finds no files deletes nothing.
    ///
    /// # Errors
    ///
    /// Returns an error when the scan or a database call fails. The old
    /// folder stays current in that case.
    pub fn open_folder(&mut self, folder: &Path) -> Result<Opened, PipelineError> {
        let files = build_pool()
            .map_err(|error| PipelineError::ScanPool(error.to_string()))?
            .install(|| scan_folder(folder))?;
        let folder_text = folder.to_string_lossy().replace('\\', "/");
        self.db.write_scan_group(&files)?;
        let uuid = self
            .db
            .get_or_create_folder_uuid(&folder_text, &new_folder_id())?;
        let rows: HashMap<String, _> = self
            .db
            .thumbnails_for_folder(&folder_text)?
            .into_iter()
            .map(|row| (row.path.clone(), row))
            .collect();
        // An empty scan prunes nothing: a missing folder and an unmounted
        // mount point both scan as empty.
        prune::prune_missing(&self.db, &self.cache_root, folder, &rows, &files);
        let color_state = ColorState::read(&self.db);
        let folder_name = folder.file_name().map_or_else(
            || "root".to_owned(),
            |name| name.to_string_lossy().into_owned(),
        );

        let mut failures = self
            .failures
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        // The generation moves under the failures lock, so a worker never
        // records a failure of an old job after the retried set is reset.
        let generation = self.generation.fetch_add(1, Ordering::AcqRel) + 1;
        self.retried
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clear();
        // Drop failures of files that are not in this folder.
        let present: HashSet<&PathBuf> = files.iter().map(|file| &file.path).collect();
        failures.retain(|path, _| present.contains(path));
        let jobs: Vec<Job> = files
            .iter()
            .map(|file| {
                let stamp = SourceStamp {
                    mtime: file.mtime,
                    size: file.size,
                };
                let key = file.path.to_string_lossy().replace('\\', "/");
                let stored = rows
                    .get(&key)
                    .filter(|row| row.thumbnail_cache_path.is_some())
                    .map(|row| SourceStamp {
                        mtime: row.mtime,
                        size: u64::try_from(row.size).unwrap_or(0),
                    });
                let known_failure = failures
                    .get(&file.path)
                    .filter(|(failed, _)| *failed == stamp)
                    .map(|(_, error)| error.clone());
                let (colors, needs_colors) = color_state.for_path(&key);
                Job {
                    generation,
                    path: file.path.clone(),
                    stamp,
                    stored,
                    folder: folder_name.clone(),
                    uuid: uuid.clone(),
                    known_failure,
                    colors,
                    needs_colors,
                }
            })
            .collect();
        drop(failures);
        self.jobs = jobs
            .iter()
            .map(|job| (job.path.clone(), job.clone()))
            .collect();
        self.queue.replace(jobs);
        Ok(Opened { generation, files })
    }

    /// The cache root the pipeline writes to.
    #[must_use]
    pub fn cache_root(&self) -> &Path {
        &self.cache_root
    }

    /// The database the pipeline stores its rows in.
    #[must_use]
    pub fn database(&self) -> &Database {
        &self.db
    }

    /// Queue these files again, ahead of the rest. Use it for a visible tile
    /// whose texture was dropped. The render step reads the cache, so this is
    /// cheap when the first render stored its files. Unknown paths are skipped.
    pub fn request(&self, keys: &HashSet<PathBuf>) {
        let jobs = keys
            .iter()
            .filter_map(|key| self.jobs.get(key))
            .map(|job| Job {
                // The first render stored the stamp it saw, so the cache check
                // passes when the files exist and falls back to a render if not.
                stored: Some(job.stamp),
                // A request reloads a texture; tagging stays with the open that built the job.
                needs_colors: false,
                ..job.clone()
            })
            .collect();
        self.queue.request(jobs);
    }

    /// Render these files again, ahead of the rest, and forget their failures.
    /// Use it when the user asks to retry a failed file. A plain
    /// [`Self::request`] would return the remembered failure and the cache
    /// check could serve an old file, so the retry skips both.
    ///
    /// A job of the file that is still waiting is replaced by the new one. A
    /// render that is already running is not stopped; the retry queues a second
    /// render, and an error of the running one is neither sent nor remembered.
    /// Unknown paths are skipped. It returns how many jobs it queued.
    pub fn retry(&mut self, keys: &HashSet<PathBuf>) -> usize {
        let mut failures = self
            .failures
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut retried = self
            .retried
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut jobs = Vec::new();
        for key in keys {
            let Some(job) = self.jobs.get_mut(key) else {
                continue;
            };
            failures.remove(key);
            *retried.entry(key.clone()).or_default() += 1;
            // The stored job loses the failure too, so a later request renders.
            job.known_failure = None;
            jobs.push(Job {
                stored: None,
                needs_colors: false,
                ..job.clone()
            });
        }
        drop(retried);
        drop(failures);
        self.queue.retry(jobs)
    }

    /// Queue the jobs for these visible items first. Other jobs keep their
    /// scan order behind them.
    pub fn reprioritise(&self, visible: &HashSet<PathBuf>) {
        self.queue.reprioritise(visible);
    }

    /// Drop every queued job and wait until no worker is rendering.
    ///
    /// Results of the current jobs are dropped too. The workers stay alive, so
    /// a later open works. A running render is not stopped, so the wait can be
    /// long: call this off the UI thread.
    pub fn drain(&mut self) {
        self.generation.fetch_add(1, Ordering::AcqRel);
        self.queue.replace(Vec::new());
        self.jobs.clear();
        self.queue.wait_idle();
    }

    /// Wait until the writer has stored every row queued so far.
    pub fn flush(&self) {
        let Some(rows) = &self.rows else { return };
        let (ack_tx, ack_rx) = crossbeam_channel::bounded(1);
        if rows.send(writer::Message::Flush(ack_tx)).is_ok() {
            let _ = ack_rx.recv();
        }
    }
}

impl Drop for Pipeline {
    fn drop(&mut self) {
        self.queue.shutdown();
        for worker in self.workers.drain(..) {
            let _ = worker.join();
        }
        // The workers held the other senders. With this one gone the writer
        // stores what is left and ends.
        self.rows = None;
        if let Some(writer) = self.writer.take() {
            let _ = writer.join();
        }
    }
}

/// A new folder id: time, process id, and a counter in hex.
fn new_folder_id() -> String {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |age| age.as_nanos());
    let count = NEXT.fetch_add(1, Ordering::Relaxed);
    format!("{nanos:024x}{:08x}{count:04x}", std::process::id())
}
