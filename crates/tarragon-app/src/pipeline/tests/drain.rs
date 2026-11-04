//! Tests for `Pipeline::drain`.

use std::sync::atomic::AtomicUsize;

use crossbeam_channel::{Receiver, Sender};

use super::*;

/// How long a drain that must wait is given to show that it does not return.
const STILL_WAITING: Duration = Duration::from_millis(200);

/// How long a drain that must return is given before the test fails.
const DRAIN_WAIT: Duration = Duration::from_secs(10);

/// What the test keeps of a render that blocks on its first call.
struct Gate {
    entered: Receiver<()>,
    release: Sender<()>,
    calls: Arc<AtomicUsize>,
}

/// A render function whose first call announces itself and waits for the
/// release. Every call is counted. With `panic` the first call panics after
/// the release.
fn gated_render(panic: bool) -> (RenderFn, Gate) {
    let (entered_tx, entered) = crossbeam_channel::unbounded::<()>();
    let (release, release_rx) = crossbeam_channel::unbounded::<()>();
    let calls = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&calls);
    let render: RenderFn = Arc::new(move |_| {
        if counter.fetch_add(1, Ordering::SeqCst) == 0 {
            entered_tx.send(()).expect("signal");
            release_rx.recv().expect("release");
            assert!(!panic, "the render fails on purpose");
        }
        Ok(DynamicImage::ImageRgba8(RgbaImage::new(4, 4)))
    });
    (
        render,
        Gate {
            entered,
            release,
            calls,
        },
    )
}

/// Open a folder of `count` files on a one-worker pipeline whose first render
/// is held at the gate.
fn open_held(count: usize, panic: bool) -> (Setup, Gate) {
    let dir = TempDir::new();
    let folder = dir.sub("held");
    for index in 0..count {
        write_png(&folder.join(format!("f{index:02}.png")), 255);
    }
    let (render, gate) = gated_render(panic);
    let mut setup = start(dir, 1, render);
    setup
        .pipeline
        .open_folder(&folder)
        .expect("open the folder");
    gate.entered.recv_timeout(WAIT).expect("a render started");
    (setup, gate)
}

/// Call `drain` on another thread while a render is held. Check that it has
/// not returned once it has started, release the render, and check that it
/// then returns.
fn drain_across_release(setup: &mut Setup, gate: &Gate) {
    let generation = Arc::clone(&setup.pipeline.generation);
    let before = generation.load(Ordering::Acquire);
    let queue = Arc::clone(&setup.pipeline.queue);
    let (done_tx, done_rx) = crossbeam_channel::bounded::<()>(1);
    std::thread::scope(|scope| {
        scope.spawn(|| {
            setup.pipeline.drain();
            done_tx.send(()).expect("signal");
        });
        // The generation moves first in `drain`, so the call is under way.
        let start = std::time::Instant::now();
        while generation.load(Ordering::Acquire) == before && start.elapsed() < DRAIN_WAIT {
            std::thread::yield_now();
        }
        let started = generation.load(Ordering::Acquire) != before;
        let early = done_rx.recv_timeout(STILL_WAITING).is_ok();
        // The release comes before any assert, so a failed check cannot leave
        // the render held and the scope waiting for the thread.
        let _ = gate.release.send(());
        let late = early || done_rx.recv_timeout(DRAIN_WAIT).is_ok();
        if !late {
            // A drain that never returns would keep the scope from ending.
            queue.shutdown();
        }
        assert!(started, "drain moves the generation");
        assert!(!early, "drain waits for the running render");
        assert!(late, "drain returns after the release");
    });
}

#[test]
fn drain_waits_for_a_running_render() {
    let (mut setup, gate) = open_held(1, false);
    drain_across_release(&mut setup, &gate);
}

#[test]
fn drain_leaves_no_queued_job() {
    let (mut setup, gate) = open_held(20, false);
    drain_across_release(&mut setup, &gate);
    assert_eq!(
        gate.calls.load(Ordering::SeqCst),
        1,
        "only the running file"
    );
}

#[test]
fn a_panicking_render_releases_its_slot() {
    let (mut setup, gate) = open_held(3, true);
    drain_across_release(&mut setup, &gate);
}

#[test]
fn a_drained_result_never_reaches_the_consumer() {
    let (mut setup, gate) = open_held(1, false);
    drain_across_release(&mut setup, &gate);
    assert!(setup.results.try_recv().is_none());
}

#[test]
fn drain_on_an_idle_pipeline_returns_and_a_later_open_renders() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let dir = TempDir::new();
    let folder = dir.sub("later");
    write_png(&folder.join("a.png"), 255);
    let mut setup = start(dir, 2, counting_render(&calls));

    let queue = Arc::clone(&setup.pipeline.queue);
    let (done_tx, done_rx) = crossbeam_channel::bounded::<()>(1);
    std::thread::scope(|scope| {
        scope.spawn(|| {
            setup.pipeline.drain();
            done_tx.send(()).expect("signal");
        });
        let returned = done_rx.recv_timeout(DRAIN_WAIT).is_ok();
        if !returned {
            // A drain that never returns would keep the scope from ending.
            queue.shutdown();
        }
        assert!(returned, "an idle drain returns");
    });

    setup
        .pipeline
        .open_folder(&folder)
        .expect("open the folder");
    assert_eq!(collect(&setup.results, 1).len(), 1);
    assert_eq!(calls.lock().expect("calls lock").len(), 1);
}
