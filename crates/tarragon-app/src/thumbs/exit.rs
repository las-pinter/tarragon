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

impl Slot {
    /// Wait for the pipeline and drop it, which joins its workers.
    fn stop(self) {
        match self {
            Slot::Idle(pipeline) => drop(pipeline),
            // The open thread returns the pipeline; dropping it ends the work.
            Slot::Opening { done, .. } => drop(done.recv()),
            Slot::Working { done, .. } => drop(done.recv()),
            Slot::Gone => {}
        }
    }
}

impl Drop for Thumbs {
    /// An app that dies without `shutdown` (a failed window) still stops its
    /// pipeline here. Otherwise a thread in flight would hold the database
    /// and cache after the app is gone.
    fn drop(&mut self) {
        std::mem::replace(&mut self.slot, Slot::Gone).stop();
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
                slot.stop();
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

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::{Duration, Instant};

    use eframe::egui::Context;
    use image::{DynamicImage, RgbaImage};

    use super::super::Work;
    use super::super::tests::Rig;
    use super::*;

    /// How long a step that must finish is given before the test fails.
    const STEP_WAIT: Duration = Duration::from_secs(10);

    #[test]
    fn waiting_twice_returns_both_times() {
        let wait = ExitWait::default();
        let handle = std::thread::spawn(|| {});
        *wait.0.lock().expect("lock") = Some(handle);
        wait.wait();
        wait.wait();
        assert!(wait.0.lock().expect("lock").is_none());
    }

    #[test]
    fn dropping_thumbs_during_work_waits_for_the_work_thread() {
        let armed = Arc::new(AtomicBool::new(false));
        let (entered_tx, entered) = crossbeam_channel::unbounded::<()>();
        let (release, release_rx) = crossbeam_channel::unbounded::<()>();
        let gate = Arc::clone(&armed);
        let render: crate::pipeline::RenderFn = Arc::new(move |_| {
            if gate.load(Ordering::SeqCst) {
                let _ = entered_tx.send(());
                // The sender is only dropped, never used: the drop is the release.
                let _ = release_rx.recv();
            }
            Ok(DynamicImage::ImageRgba8(RgbaImage::new(8, 8)))
        });
        let mut rig = Rig::with_render("drop-waits", Some("one"), render);
        // Declared after the rig, so a failed assert drops it first and the
        // held render ends before the rig waits for it.
        let release = release;
        rig.settle();
        let two = rig.folder("two", &["t1.png", "t2.png"]);
        armed.store(true, Ordering::SeqCst);
        rig.thumbs.navigate(two);
        let start = Instant::now();
        let mut queued = false;
        // A held render keeps the purge on its work thread.
        while rig.thumbs.is_working() != Some(Work::Purge) {
            rig.thumbs
                .before_draw(&Context::default(), &mut rig.gallery);
            if entered.try_recv().is_ok() {
                rig.thumbs.purge(rig.dir.0.join("data"));
                queued = true;
            }
            assert!(start.elapsed() < STEP_WAIT, "the purge never started");
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(queued);

        let (done_tx, done_rx) = crossbeam_channel::bounded::<()>(1);
        let thumbs = rig.thumbs;
        let dropper = std::thread::spawn(move || {
            drop(thumbs);
            let _ = done_tx.send(());
        });
        let early = done_rx.recv_timeout(Duration::from_millis(300)).is_ok();
        // The release comes before any assert, so a failed check cannot hang.
        armed.store(false, Ordering::SeqCst);
        drop(release);
        let late = done_rx.recv_timeout(STEP_WAIT).is_ok();
        let _ = dropper.join();
        assert!(!early, "the drop returned while the work still ran");
        assert!(late, "the drop never returned after the release");
    }
}
