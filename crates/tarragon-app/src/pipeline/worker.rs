//! The worker loop and the per-file flow.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use crossbeam_channel::{SendTimeoutError, Sender};
use image::{DynamicImage, RgbaImage};
use tarragon_cache::store::{CacheEntry, CacheStore};
use tarragon_cache::thumb_image;
use tarragon_db::repository::ThumbnailWrite;

use super::job::{ErrorKind, Job, ThumbError, ThumbResult};
use super::queue::JobQueue;
use super::writer::Message;
use super::{Failures, RenderFn, Repaint};

/// How long a full channel is waited on before the checks run again.
const SEND_WAIT: Duration = Duration::from_millis(20);

/// What a worker shares with the pipeline.
pub(super) struct Shared {
    pub(super) queue: Arc<JobQueue>,
    pub(super) generation: Arc<AtomicU64>,
    pub(super) cache: CacheStore,
    pub(super) render: RenderFn,
    pub(super) failures: Failures,
    pub(super) results: Sender<ThumbResult>,
    pub(super) rows: Sender<Message>,
    pub(super) repaint: Repaint,
}

/// Take jobs until the queue shuts down.
pub(super) fn run(shared: &Shared) {
    while let Some(job) = shared.queue.next() {
        // A job from an older folder is dropped before any work starts.
        if job.generation != shared.generation.load(Ordering::Acquire) {
            continue;
        }
        let outcome =
            catch_unwind(AssertUnwindSafe(|| process(shared, &job))).unwrap_or_else(|_| {
                Err(ThumbError {
                    kind: ErrorKind::Panicked,
                    message: "the worker panicked".to_owned(),
                })
            });
        if let Some(error) = outcome.as_ref().err().filter(|e| e.kind.is_permanent()) {
            shared
                .failures
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .insert(job.path.clone(), (job.stamp, error.clone()));
        }
        // The folder may have changed while the file rendered.
        if job.generation != shared.generation.load(Ordering::Acquire) {
            continue;
        }
        let result = ThumbResult {
            generation: job.generation,
            key: job.path,
            outcome,
        };
        match send(shared, result) {
            Sent::Yes => {
                // A panic in the callback must not kill the worker.
                let _ = catch_unwind(AssertUnwindSafe(|| (shared.repaint)()));
            }
            Sent::Dropped => {}
            Sent::Gone => return,
        }
    }
}

/// How a send ended.
enum Sent {
    Yes,
    /// The result went stale or the pipeline is closing.
    Dropped,
    /// The consumer is gone.
    Gone,
}

/// Send a result without blocking forever. The channel is bounded, so a
/// consumer that stops reading must not hold a worker: the wait re-checks the
/// generation and the shutdown flag every few milliseconds.
fn send(shared: &Shared, mut result: ThumbResult) -> Sent {
    loop {
        match shared.results.send_timeout(result, SEND_WAIT) {
            Ok(()) => return Sent::Yes,
            Err(SendTimeoutError::Disconnected(_)) => return Sent::Gone,
            Err(SendTimeoutError::Timeout(back)) => {
                if back.generation != shared.generation.load(Ordering::Acquire)
                    || shared.queue.is_shutdown()
                {
                    return Sent::Dropped;
                }
                result = back;
            }
        }
    }
}

/// Serve one file from the cache, or render it.
fn process(shared: &Shared, job: &Job) -> Result<RgbaImage, ThumbError> {
    if let Some(error) = &job.known_failure {
        return Err(error.clone());
    }
    let name = job
        .path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| ThumbError {
            kind: ErrorKind::InvalidName,
            message: "the file name is not UTF-8".to_owned(),
        })?;
    if let Some(image) = from_cache(shared, job, name) {
        // Eviction orders by this time, so a hit counts as an access.
        let path = job.path.to_string_lossy().replace('\\', "/");
        let _ = shared.rows.send(Message::Touch(path));
        return Ok(image);
    }
    let image = (shared.render)(&job.path).map_err(|error| ThumbError {
        kind: ErrorKind::from(&error),
        message: error.to_string(),
    })?;
    match shared.cache.write_all(&image, &job.folder, &job.uuid, name) {
        Ok(entry) => {
            // The row goes first, so a received result means a queued row.
            let _ = shared.rows.send(Message::Row(row_for(job, &image, &entry)));
        }
        // The image is fine; only the cache failed, so the cell still shows.
        Err(error) => tracing::warn!("cannot cache {}: {error}", job.path.display()),
    }
    Ok(thumb_image(&image).to_rgba8())
}

/// Load the 256 tier when the row and the files are fresh.
///
/// A file that does not decode is a miss, because a power loss can leave a
/// partial file that is not empty.
fn from_cache(shared: &Shared, job: &Job, name: &str) -> Option<RgbaImage> {
    let stored = job.stored?;
    let entry = shared
        .cache
        .find_entry(&job.folder, &job.uuid, name)
        .ok()??;
    if !entry.is_fresh(stored, job.stamp) {
        return None;
    }
    match image::open(&entry.thumb256) {
        Ok(image) => Some(image.to_rgba8()),
        Err(error) => {
            tracing::warn!("bad cache file {}: {error}", entry.thumb256.display());
            None
        }
    }
}

/// Build the row for a fresh render.
///
/// The stamp is the one taken at scan time, before the render, on purpose. If
/// the source changed during the render, the next open sees a new stamp and
/// renders again.
fn row_for(job: &Job, image: &DynamicImage, entry: &CacheEntry) -> ThumbnailWrite {
    let text = |path: &std::path::Path| Some(path.to_string_lossy().replace('\\', "/"));
    ThumbnailWrite {
        path: job.path.to_string_lossy().replace('\\', "/"),
        mtime: job.stamp.mtime,
        size: i64::try_from(job.stamp.size).unwrap_or(i64::MAX),
        width: image.width(),
        height: image.height(),
        cache_uuid: job.uuid.clone(),
        thumbnail_cache_path: text(&entry.thumb256),
        preview_cache_path: text(&entry.preview1024),
        full_cache_path: text(&entry.full),
        last_accessed: super::writer::now(),
    }
}
