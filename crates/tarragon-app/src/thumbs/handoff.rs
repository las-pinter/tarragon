//! Run pipeline work on a short-lived thread and always get the pipeline back.
//!
//! The pipeline moves into the thread. A panic in the work is caught, so the
//! pipeline still comes back and the app keeps its thumbnails. A thread that
//! does not start hands the pipeline back with the error.
//!
//! After a panic the pipeline comes back as the work left it, for example with
//! half a folder queued. The next folder open replaces that state. The panic
//! hook still writes a crash report, so the fault is not lost.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{Arc, Mutex, PoisonError};

use crossbeam_channel::Receiver;

use crate::pipeline::{Pipeline, Repaint};

/// What a pipeline thread sends back.
pub(super) struct Answer<R> {
    /// The pipeline, ready for the next call.
    pub(super) pipeline: Pipeline,
    /// The answer of the work, or none when the work panicked.
    pub(super) result: Option<R>,
}

/// A thread that did not start.
pub(super) struct NotStarted {
    /// The pipeline, which the thread never took.
    pub(super) pipeline: Option<Pipeline>,
    /// Why the thread did not start.
    pub(super) error: std::io::Error,
}

/// Start `work` on a new thread called `name`, with the pipeline.
///
/// The answer comes on the returned channel, also after a panic in `work`.
/// `repaint` runs after the answer is sent, so the UI takes it.
///
/// # Errors
///
/// When the thread does not start, returns the pipeline and the error. The
/// pipeline is always there then, because the thread never took it.
pub(super) fn spawn<R: Send + 'static>(
    name: &str,
    pipeline: Pipeline,
    repaint: Repaint,
    work: impl FnOnce(&mut Pipeline) -> R + Send + 'static,
) -> Result<Receiver<Answer<R>>, Box<NotStarted>> {
    let holder = Arc::new(Mutex::new(Some(pipeline)));
    let inner = Arc::clone(&holder);
    let (sender, done) = crossbeam_channel::bounded(1);
    let thread_name = name.to_owned();
    let spawned = std::thread::Builder::new()
        .name(name.to_owned())
        .spawn(move || {
            let taken = inner.lock().unwrap_or_else(PoisonError::into_inner).take();
            let Some(mut pipeline) = taken else {
                return;
            };
            let result = catch_unwind(AssertUnwindSafe(|| work(&mut pipeline))).ok();
            if result.is_none() {
                tracing::error!("the {thread_name} thread panicked; the pipeline is kept");
            }
            // A closed window drops the answer, and the pipeline with it.
            let _ = sender.send(Answer { pipeline, result });
            repaint();
        });
    match spawned {
        Ok(_) => Ok(done),
        Err(error) => {
            let pipeline = holder.lock().unwrap_or_else(PoisonError::into_inner).take();
            Err(Box::new(NotStarted { pipeline, error }))
        }
    }
}

#[cfg(test)]
#[path = "handoff_tests.rs"]
mod tests;
