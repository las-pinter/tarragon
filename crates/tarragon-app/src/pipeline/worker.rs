//! The worker loop and the per-file flow.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use crossbeam_channel::{SendTimeoutError, Sender};
use image::{DynamicImage, RgbaImage};
use tarragon_cache::store::{CacheEntry, CacheStore};
use tarragon_cache::thumb_image;
use tarragon_db::repository::ThumbnailWrite;
use tarragon_render::color::{ColorTagConfig, ColorTags};

use super::job::{ErrorKind, Job, ThumbError, ThumbResult};
use super::queue::JobQueue;
use super::writer::Message;
use super::{Failures, RenderFn, Repaint, Retried};
use crate::colortags;

/// How long a full channel is waited on before the checks run again.
const SEND_WAIT: Duration = Duration::from_millis(20);

/// What a worker shares with the pipeline.
pub(super) struct Shared {
    pub(super) queue: Arc<JobQueue>,
    pub(super) generation: Arc<AtomicU64>,
    pub(super) cache: CacheStore,
    pub(super) render: RenderFn,
    pub(super) failures: Failures,
    pub(super) retried: Retried,
    pub(super) results: Sender<ThumbResult>,
    pub(super) rows: Sender<Message>,
    pub(super) repaint: Repaint,
    pub(super) extract: ColorFn,
}

/// The colour extraction function. Tests swap it to force a panic.
pub(super) type ColorFn = fn(&DynamicImage, ColorTagConfig) -> ColorTags;

/// Releases the queue slot of a taken job when it drops, so every exit from
/// the loop body (skip, panic, early return) frees the slot.
struct Slot<'a>(&'a JobQueue, PathBuf);

impl Drop for Slot<'_> {
    fn drop(&mut self) {
        self.0.finish(&self.1);
    }
}

/// Take jobs until the queue shuts down.
pub(super) fn run(shared: &Shared) {
    while let Some(job) = shared.queue.next() {
        let _slot = Slot(&shared.queue, job.path.clone());
        // A job from an older folder is dropped before any work starts.
        if job.generation != shared.generation.load(Ordering::Acquire) {
            continue;
        }
        let asked = asked_count(shared, &job);
        let outcome =
            catch_unwind(AssertUnwindSafe(|| process(shared, &job))).unwrap_or_else(|_| {
                Err(ThumbError {
                    kind: ErrorKind::Panicked,
                    message: "the worker panicked".to_owned(),
                })
            });
        // The folder may have changed while the file rendered.
        if !record(shared, &job, &outcome, asked) {
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

/// Remember a permanent failure of the job, or forget the failure of a file
/// that now renders. It returns false when the result must not be sent: the
/// job is from an old generation, or it failed while a retry of its file was
/// asked for.
///
/// The generation check and the edit share the failures lock, so an open
/// cannot slip between them. `asked` is the retry count of the file when the
/// job was taken. If the count moved, a newer render exists and this error is
/// dropped, so it cannot show a failure over a newer result that worked. A
/// later job that fails without a retry in between is kept.
fn record(shared: &Shared, job: &Job, outcome: &Result<RgbaImage, ThumbError>, asked: u64) -> bool {
    let mut failures = shared
        .failures
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if job.generation != shared.generation.load(Ordering::Acquire) {
        return false;
    }
    match outcome {
        Ok(_) => {
            failures.remove(&job.path);
        }
        Err(error) => {
            if asked_count(shared, job) != asked {
                return false;
            }
            if error.kind.is_permanent() {
                failures.insert(job.path.clone(), (job.stamp, error.clone()));
            }
        }
    }
    true
}

/// How often the file of `job` was asked for again since the last open.
fn asked_count(shared: &Shared, job: &Job) -> u64 {
    shared
        .retried
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(&job.path)
        .copied()
        .unwrap_or(0)
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
                    || shared.queue.is_settling(&back.key)
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
        .ok_or_else(|| {
            let error = ThumbError {
                kind: ErrorKind::InvalidName,
                message: "the file name is not UTF-8".to_owned(),
            };
            log_failure(&job.path, &error);
            error
        })?;
    if let Some(image) = from_cache(shared, job, name) {
        // Eviction orders by this time, so a hit counts as an access.
        let path = job.path.to_string_lossy().replace('\\', "/");
        let _ = shared.rows.send(Message::Touch(path));
        let image = DynamicImage::ImageRgba8(image);
        if job.needs_colors {
            send_colors(shared, job, &image);
        }
        return Ok(image.into_rgba8());
    }
    let image = (shared.render)(&job.path).map_err(|error| {
        let error = ThumbError {
            kind: ErrorKind::from(&error),
            message: error.to_string(),
        };
        log_failure(&job.path, &error);
        error
    })?;
    match shared.cache.write_all(&image, &job.folder, &job.uuid, name) {
        Ok(entry) => {
            // The row goes first, so a received result means a queued row.
            let _ = shared.rows.send(Message::Row(row_for(job, &image, &entry)));
        }
        // The image is fine; only the cache failed, so the cell still shows.
        Err(error) => tracing::warn!("cannot cache {}: {error}", job.path.display()),
    }
    let thumb = thumb_image(&image);
    send_colors(shared, job, &thumb);
    Ok(thumb.into_rgba8())
}

/// Write the log line the error card points to. A stopped render is not a
/// failure of the file, so it writes nothing.
fn log_failure(path: &Path, error: &ThumbError) {
    if error.kind != ErrorKind::Cancelled {
        tracing::error!("cannot render {}: {}", path.display(), error.message);
    }
}

/// Extract the colour tags of the 320 tier image and queue them for the
/// writer. It sends nothing when colour tagging is off or the extraction
/// panics; the thumbnail is not affected either way.
fn send_colors(shared: &Shared, job: &Job, thumb: &DynamicImage) {
    let Some(config) = job.colors else { return };
    let Ok(tags) = catch_unwind(AssertUnwindSafe(|| (shared.extract)(thumb, config))) else {
        tracing::warn!("colour extraction panicked for {}", job.path.display());
        return;
    };
    let path = job.path.to_string_lossy().replace('\\', "/");
    let names = colortags::tag_names(&tags);
    let _ = shared.rows.send(Message::Colors { path, names });
}

/// Load the 320 tier when the row and the files are fresh.
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
    match image::open(&entry.thumb320) {
        Ok(image) => Some(image.to_rgba8()),
        Err(error) => {
            tracing::warn!("bad cache file {}: {error}", entry.thumb320.display());
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
        thumbnail_cache_path: text(&entry.thumb320),
        preview_cache_path: text(&entry.preview1024),
        full_cache_path: text(&entry.full),
        last_accessed: super::writer::now(),
    }
}
