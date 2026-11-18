//! Tests for the purge and the eviction on the work thread.

use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;

use crossbeam_channel::Sender;

use super::tests::Rig;
use super::*;

/// How long a step that must finish is given before the test fails.
const STEP_WAIT: Duration = Duration::from_secs(10);

/// A rig whose renders can be held. While armed, every render announces
/// itself and waits for the release. Dropping the rig releases them, so a
/// failed assert cannot leave a render blocked.
struct Held {
    rig: Rig,
    armed: Arc<AtomicBool>,
    entered: Receiver<()>,
    release: Option<Sender<()>>,
}

impl Held {
    fn new(name: &str, start: Option<&str>) -> Self {
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
            Ok(image::DynamicImage::ImageRgba8(RgbaImage::new(8, 8)))
        });
        Self {
            rig: Rig::with_render(name, start, render),
            armed,
            entered,
            release: Some(release),
        }
    }

    fn arm(&self) {
        self.armed.store(true, Ordering::SeqCst);
    }

    /// Let every held and later render go on.
    fn release(&mut self) {
        self.armed.store(false, Ordering::SeqCst);
        self.release.take();
    }

    /// Run frames until `done` is true.
    fn pump_until(&mut self, mut done: impl FnMut(&mut Rig) -> bool) {
        let start = Instant::now();
        loop {
            self.rig
                .thumbs
                .before_draw(&self.rig.ctx, &mut self.rig.gallery);
            if done(&mut self.rig) {
                return;
            }
            assert!(start.elapsed() < STEP_WAIT, "the step never finished");
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// Open a second folder whose renders are held, then queue a purge and
    /// run frames until the purge is on its thread, waiting for the renders.
    fn purge_behind_a_held_render(&mut self) -> PathBuf {
        self.rig.settle();
        let two = self.rig.folder("two", &["t1.png", "t2.png"]);
        self.arm();
        self.rig.thumbs.navigate(two.clone());
        let entered = self.entered.clone();
        self.pump_until(|_| entered.try_recv().is_ok());
        self.pump_until(|rig| {
            if rig.thumbs.is_working().is_none() && rig.thumbs.queued.is_none() {
                rig.thumbs.purge(rig.dir.0.join("data"));
            }
            rig.thumbs.is_working() == Some(Work::Purge)
        });
        two
    }
}

impl Drop for Held {
    fn drop(&mut self) {
        self.release();
    }
}

/// The regular files under `dir`, at any depth.
fn files_under(dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return found;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            found.extend(files_under(&path));
        } else {
            found.push(path);
        }
    }
    found
}

#[test]
fn a_purge_runs_on_a_thread_and_the_folder_opens_again() {
    let mut held = Held::new("purge", Some("one"));
    held.rig.settle();
    let cache = held.rig.dir.0.join("cache");
    assert!(!files_under(&cache).is_empty(), "the open filled the cache");

    // The reopened tiles wait at the gate, so the cache stays empty to check.
    held.arm();
    held.rig.thumbs.purge(held.rig.dir.0.join("data"));
    held.rig
        .thumbs
        .before_draw(&held.rig.ctx, &mut held.rig.gallery);
    assert_eq!(held.rig.thumbs.is_working(), Some(Work::Purge));
    let mut result = None;
    held.pump_until(|rig| {
        result = rig.thumbs.take_work_result();
        result.is_some()
    });

    let Some((Work::Purge, Ok(WorkOutcome::Purged(outcome)))) = result else {
        panic!("the purge gave {result:?}");
    };
    assert!(outcome.files_removed > 0);
    assert!(files_under(&cache).is_empty(), "no tier file is left");
    assert!(held.rig.thumbs.take_work_result().is_none(), "only once");
    assert_eq!(held.rig.thumbs.is_working(), None);

    held.release();
    let summary = held.rig.settle();
    assert!(
        summary.starts_with("1 tiles settled, 0 failed"),
        "{summary}"
    );
    assert!(!files_under(&cache).is_empty(), "the tiles rendered again");
}

#[test]
fn a_purge_reopens_the_library_and_leaves_the_folder_alone() {
    let mut held = Held::new("library", None);
    let a = held.rig.folder("a", &["a1.png", "a2.png"]);
    let b = held.rig.folder("b", &["b1.png"]);
    for folder in [&b, &a] {
        held.rig.thumbs.navigate(folder.clone());
        held.rig.settle();
    }
    held.rig.thumbs.show_library();
    held.pump_until(|rig| rig.thumbs.library_loaded() && rig.thumbs.feed.all_settled());

    assert!(held.rig.gallery.has_item(&b.join("b1.png")));

    held.rig.thumbs.purge(held.rig.dir.0.join("data"));
    let mut result = None;
    held.pump_until(|rig| {
        result = rig.thumbs.take_work_result();
        result.is_some()
    });
    assert!(matches!(result, Some((Work::Purge, Ok(_)))), "{result:?}");
    held.pump_until(|rig| rig.thumbs.library_loaded() && rig.thumbs.feed.all_settled());

    let thumbs = &held.rig.thumbs;
    assert_eq!(thumbs.current.as_deref(), Some(a.as_path()));
    assert_eq!(thumbs.pending, None);
    // The purge keeps the file rows, so the reopened library lists every file.
    for name in ["a1.png", "a2.png"] {
        assert!(held.rig.gallery.has_item(&a.join(name)), "{name}");
    }
    assert!(held.rig.gallery.has_item(&b.join("b1.png")));
    assert!(thumbs.feed.all_settled());
    let cache = held.rig.dir.0.join("cache");
    assert!(!files_under(&cache).is_empty(), "the tiles rendered again");
}

#[test]
fn a_refused_purge_reports_the_error_and_the_folder_still_opens() {
    let mut held = Held::new("refused", Some("one"));
    held.rig.settle();
    let cache = held.rig.dir.0.join("cache");
    let before = files_under(&cache).len();

    // A data root equal to the cache root is refused.
    held.rig.thumbs.purge(cache.clone());
    let mut result = None;
    held.pump_until(|rig| {
        result = rig.thumbs.take_work_result();
        result.is_some()
    });

    let Some((Work::Purge, Err(message))) = result else {
        panic!("the purge gave {result:?}");
    };
    assert!(message.contains("unsafe"), "{message}");
    assert_eq!(files_under(&cache).len(), before, "nothing was deleted");
    let summary = held.rig.settle();
    assert!(
        summary.starts_with("1 tiles settled, 0 failed"),
        "{summary}"
    );
}

#[test]
fn a_work_request_waits_for_a_running_open() {
    let mut held = Held::new("waits", Some("one"));
    held.rig.settle();
    let two = held.rig.folder("two", &["t1.png"]);
    held.rig.thumbs.navigate(two.clone());
    held.rig
        .thumbs
        .before_draw(&held.rig.ctx, &mut held.rig.gallery);
    held.rig.thumbs.purge(held.rig.dir.0.join("data"));
    assert_eq!(held.rig.thumbs.is_working(), None, "the open runs first");

    held.pump_until(|rig| rig.thumbs.take_work_result().is_some());
    held.rig.settle();
    assert_eq!(held.rig.gallery.folder(), Some(two.as_path()));
}

#[test]
fn a_folder_request_waits_for_running_work() {
    let mut held = Held::new("folder", Some("one"));
    let two = held.purge_behind_a_held_render();
    let three = held.rig.folder("three", &["h1.png"]);
    held.rig.thumbs.navigate(three.clone());
    for _ in 0..3 {
        held.rig
            .thumbs
            .before_draw(&held.rig.ctx, &mut held.rig.gallery);
    }
    let waiting = held.rig.thumbs.is_working();
    let shown = held.rig.gallery.folder().map(Path::to_path_buf);
    held.release();
    assert_eq!(waiting, Some(Work::Purge), "the work still runs");
    assert_eq!(shown, Some(two), "the folder did not open yet");

    held.pump_until(|rig| rig.thumbs.take_work_result().is_some());
    held.rig.settle();
    assert_eq!(held.rig.gallery.folder(), Some(three.as_path()));
}

#[test]
fn a_regenerate_returns_at_once_and_runs_on_the_work_thread() {
    let mut held = Held::new("regen", Some("one"));
    held.rig.settle();
    let two = held.rig.folder("two", &["t1.png", "t2.png"]);
    held.arm();
    held.rig.thumbs.navigate(two.clone());
    let entered = held.entered.clone();
    held.pump_until(|_| entered.try_recv().is_ok());
    let key = two.join("t1.png");

    // A regenerate that ran on this thread would wait for the held render.
    let release = held.release.take();
    let (queued, returned) = std::thread::scope(|scope| {
        let (done_tx, done_rx) = crossbeam_channel::bounded::<bool>(1);
        let thumbs = &mut held.rig.thumbs;
        scope.spawn(move || {
            let _ = done_tx.send(thumbs.regenerate(HashSet::from([key])));
        });
        let answer = done_rx.recv_timeout(Duration::from_secs(2));
        // The release comes before any assert, so a failed check cannot leave
        // the render held and the scope waiting for the thread.
        drop(release);
        held.armed.store(false, Ordering::SeqCst);
        (answer.unwrap_or(false), answer.is_ok())
    });
    assert!(returned, "regenerate waited for the render");
    assert!(queued, "the item was queued");

    let mut result = None;
    held.pump_until(|rig| {
        result = rig.thumbs.take_regenerate_result();
        result.is_some()
    });
    assert_eq!(result, Some(Ok(1)));
    assert!(
        held.rig.thumbs.take_regenerate_result().is_none(),
        "only once"
    );
}

#[test]
fn a_regenerate_waits_for_the_work_thread_and_queues_the_item() {
    let mut held = Held::new("regenq", Some("one"));
    let two = held.purge_behind_a_held_render();
    // The purge holds the pipeline, so the item must wait in the queue.
    assert!(
        held.rig
            .thumbs
            .regenerate(HashSet::from([two.join("t1.png")]))
    );
    assert!(held.rig.thumbs.has_queued_work());
    assert_eq!(held.rig.thumbs.is_working(), Some(Work::Purge));
    held.release();
    held.pump_until(|rig| rig.thumbs.take_regenerate_result().is_some());
}

#[test]
fn a_regenerate_does_not_replace_a_waiting_purge() {
    let mut rig = Rig::new("regenkeep", None);
    rig.thumbs.purge(rig.dir.0.join("data"));
    let queued = rig
        .thumbs
        .regenerate(HashSet::from([rig.dir.0.join("x.png")]));
    assert!(!queued, "the waiting purge keeps its place");
    assert!(matches!(rig.thumbs.queued, Some(Queued::Purge { .. })));
}

#[test]
fn only_the_result_of_a_regenerate_is_taken_as_one() {
    let mut rig = Rig::new("regentake", Some("one"));
    rig.settle();
    rig.thumbs.evict(0);
    let start = Instant::now();
    while rig.thumbs.finished.is_none() {
        rig.thumbs.before_draw(&rig.ctx, &mut rig.gallery);
        assert!(start.elapsed() < STEP_WAIT, "the eviction never ended");
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(rig.thumbs.take_regenerate_result().is_none());
    assert!(matches!(
        rig.thumbs.take_work_result(),
        Some((Work::Evict, _))
    ));
}

#[test]
fn an_eviction_with_a_zero_cap_changes_nothing_and_reports() {
    let mut held = Held::new("evict", Some("one"));
    held.rig.settle();
    let cache = held.rig.dir.0.join("cache");
    let before = files_under(&cache).len();

    held.rig.thumbs.evict(0);
    let mut result = None;
    held.pump_until(|rig| {
        result = rig.thumbs.take_work_result();
        result.is_some()
    });

    let Some((Work::Evict, Ok(WorkOutcome::Evicted(evicted)))) = result else {
        panic!("the eviction gave {result:?}");
    };
    assert_eq!(evicted.rows, 0);
    assert_eq!(files_under(&cache).len(), before);
}

#[test]
fn shutdown_during_work_returns_at_once_and_the_thread_ends_after_the_work() {
    let mut held = Held::new("shutdown", Some("one"));
    held.purge_behind_a_held_render();
    let wait = ExitWait::default();
    let release = held.release.take();
    let (returned, running) = std::thread::scope(|scope| {
        let (done_tx, done_rx) = crossbeam_channel::bounded::<()>(1);
        let thumbs = &mut held.rig.thumbs;
        let wait_ref = &wait;
        scope.spawn(move || {
            thumbs.shutdown(wait_ref, None);
            let _ = done_tx.send(());
        });
        let returned = done_rx.recv_timeout(STEP_WAIT).is_ok();
        let running = wait
            .0
            .lock()
            .expect("handle lock")
            .as_ref()
            .is_some_and(|handle| !handle.is_finished());
        // The release comes before any assert, so a failed check cannot leave
        // the render held and the scope waiting for the thread.
        drop(release);
        held.armed.store(false, Ordering::SeqCst);
        (returned, running)
    });
    assert!(returned, "shutdown does not wait for the work");
    assert!(running, "the thread outlives the call while the work runs");

    let start = Instant::now();
    while start.elapsed() < STEP_WAIT
        && !wait
            .0
            .lock()
            .expect("handle lock")
            .as_ref()
            .is_some_and(JoinHandle::is_finished)
    {
        std::thread::sleep(Duration::from_millis(10));
    }
    let finished = wait
        .0
        .lock()
        .expect("handle lock")
        .as_ref()
        .is_some_and(JoinHandle::is_finished);
    assert!(finished, "the thread ends after the work");
    wait.wait();
}

#[test]
fn a_failed_work_request_logs_a_warning() {
    use crate::logging::ring::{LogRing, RingLayer};

    let mut rig = Rig::new("logfail", None);
    let cache = rig.dir.0.join("cache");
    // A missing cache folder is not refused, so it must exist.
    std::fs::create_dir_all(&cache).expect("make the cache folder");
    let Slot::Idle(pipeline) = &mut rig.thumbs.slot else {
        panic!("the pipeline should be idle");
    };
    // A data root equal to the cache root makes the purge fail.
    let queued = Queued::Purge { data_root: cache };
    let lines = LogRing::new();
    let layer = RingLayer::new(lines.clone(), time::UtcOffset::UTC);
    let subscriber =
        tracing_subscriber::layer::SubscriberExt::with(tracing_subscriber::registry(), layer);
    let result = tracing::subscriber::with_default(subscriber, || queued.run(pipeline));

    assert!(result.is_err());
    assert!(lines.snapshot().iter().any(|line| {
        line.level == tracing::Level::WARN
            && line.text.contains("Purge failed")
            && line.text.contains("unsafe")
    }));
}
