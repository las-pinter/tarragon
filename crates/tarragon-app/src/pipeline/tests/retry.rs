//! Tests for `Pipeline::retry`.

use std::sync::atomic::AtomicUsize;

use tarragon_core::error::RenderError;

use super::drain::Gate;
use super::*;

/// A render function whose first call fails with `fail` and whose later calls
/// work. Every call is counted.
fn failing_once(fail: fn(&Path) -> RenderError) -> (RenderFn, Arc<AtomicUsize>) {
    let calls = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&calls);
    let render: RenderFn = Arc::new(move |path| {
        if counter.fetch_add(1, Ordering::SeqCst) == 0 {
            return Err(fail(path));
        }
        Ok(DynamicImage::ImageRgba8(RgbaImage::new(4, 4)))
    });
    (render, calls)
}

fn decode_error(path: &Path) -> RenderError {
    RenderError::Decode {
        path: path.to_path_buf(),
        reason: "broken on purpose".to_owned(),
    }
}

fn io_error(path: &Path) -> RenderError {
    RenderError::Io {
        path: path.to_path_buf(),
        source: std::io::Error::other("unreadable on purpose"),
    }
}

/// Open a one-file folder whose first render fails, and take that result.
fn open_failed(
    fail: fn(&Path) -> RenderError,
    kind: ErrorKind,
) -> (Setup, PathBuf, PathBuf, Arc<AtomicUsize>) {
    let dir = TempDir::new();
    let folder = dir.sub("retry");
    let file = folder.join("one.png");
    write_png(&file, 255);
    let (render, calls) = failing_once(fail);
    let mut setup = start(dir, 1, render);
    setup.pipeline.open_folder(&folder).expect("open");
    let first = collect(&setup.results, 1);
    let error = first[0]
        .outcome
        .as_ref()
        .expect_err("the first render fails");
    assert_eq!(error.kind, kind);
    (setup, folder, file, calls)
}

fn stored_failure(setup: &Setup, file: &Path) -> bool {
    setup
        .pipeline
        .jobs
        .get(file)
        .is_some_and(|job| job.known_failure.is_some())
}

#[test]
fn a_permanent_failure_renders_again_after_retry() {
    let (mut setup, _, file, calls) = open_failed(decode_error, ErrorKind::Decode);
    assert!(
        setup
            .pipeline
            .failures
            .lock()
            .expect("lock")
            .contains_key(&file)
    );

    assert_eq!(setup.pipeline.retry(&HashSet::from([file.clone()])), 1);
    let again = collect(&setup.results, 1);

    assert_eq!(again[0].key, file);
    assert!(again[0].outcome.is_ok());
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[test]
fn an_io_failure_renders_again_after_retry() {
    let (mut setup, _, file, calls) = open_failed(io_error, ErrorKind::Io);
    assert!(setup.pipeline.failures.lock().expect("lock").is_empty());

    assert_eq!(setup.pipeline.retry(&HashSet::from([file])), 1);
    let again = collect(&setup.results, 1);

    assert!(again[0].outcome.is_ok());
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[test]
fn retry_forgets_the_failure_so_a_second_open_renders() {
    let (mut setup, folder, file, calls) = open_failed(decode_error, ErrorKind::Decode);
    // The second open keeps the failure in the stored job.
    setup.pipeline.open_folder(&folder).expect("open again");
    assert!(collect(&setup.results, 1)[0].outcome.is_err());
    assert!(stored_failure(&setup, &file));

    setup.pipeline.retry(&HashSet::from([file.clone()]));
    assert!(collect(&setup.results, 1)[0].outcome.is_ok());
    assert!(
        !setup
            .pipeline
            .failures
            .lock()
            .expect("lock")
            .contains_key(&file),
        "the failure entry is gone"
    );
    assert!(!stored_failure(&setup, &file), "the stored job lost it too");
    // The writer stores the row of the retry render; the next open reads it.
    setup.pipeline.flush();

    setup
        .pipeline
        .open_folder(&folder)
        .expect("open a third time");
    let reopened = collect(&setup.results, 1);
    assert!(reopened[0].outcome.is_ok(), "the old failure is not reused");
    assert_eq!(
        calls.load(Ordering::SeqCst),
        2,
        "the second open used the cache"
    );
}

#[test]
fn retry_of_an_unknown_path_queues_nothing() {
    let (mut setup, folder, _, calls) = open_failed(decode_error, ErrorKind::Decode);

    let queued = setup
        .pipeline
        .retry(&HashSet::from([folder.join("nope.png")]));

    assert_eq!(queued, 0);
    assert!(
        setup
            .results
            .recv_timeout(Duration::from_millis(200))
            .is_none(),
        "nothing was queued"
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[test]
fn retry_renders_even_when_the_cache_is_good() {
    let dir = TempDir::new();
    let folder = dir.sub("good");
    let file = folder.join("one.png");
    write_png(&file, 255);
    let calls = Arc::default();
    let mut setup = start(dir, 1, counting_render(&calls));
    setup.pipeline.open_folder(&folder).expect("open");
    collect(&setup.results, 1);
    setup.pipeline.flush();

    // A plain request is served from the cache.
    setup.pipeline.request(&HashSet::from([file.clone()]));
    collect(&setup.results, 1);
    assert_eq!(calls.lock().expect("lock").len(), 1);

    assert_eq!(setup.pipeline.retry(&HashSet::from([file])), 1);
    assert!(collect(&setup.results, 1)[0].outcome.is_ok());
    assert_eq!(calls.lock().expect("lock").len(), 2, "the retry rendered");
}

/// What one render call does.
#[derive(Clone, Copy)]
enum Step {
    Ok,
    Fail,
    /// Wait at the gate, then work.
    HoldThenOk,
    /// Wait at the gate, then fail.
    HoldThenFail,
}

/// A render function that follows `steps` by call order and works after them.
fn scripted(steps: &'static [Step]) -> (RenderFn, Gate) {
    let (entered_tx, entered) = crossbeam_channel::unbounded::<()>();
    let (release, release_rx) = crossbeam_channel::unbounded::<()>();
    let calls = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&calls);
    let render: RenderFn = Arc::new(move |path| {
        let step = steps
            .get(counter.fetch_add(1, Ordering::SeqCst))
            .copied()
            .unwrap_or(Step::Ok);
        if matches!(step, Step::HoldThenOk | Step::HoldThenFail) {
            entered_tx.send(()).expect("signal");
            release_rx.recv().expect("release");
        }
        match step {
            Step::Ok | Step::HoldThenOk => Ok(DynamicImage::ImageRgba8(RgbaImage::new(4, 4))),
            Step::Fail | Step::HoldThenFail => Err(decode_error(path)),
        }
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

fn folder_with(dir: &TempDir, names: &[&str]) -> PathBuf {
    let folder = dir.sub("scripted");
    for name in names {
        write_png(&folder.join(name), 255);
    }
    folder
}

#[test]
fn retry_replaces_a_job_that_is_still_waiting() {
    let dir = TempDir::new();
    let folder = folder_with(&dir, &["a.png", "b.png"]);
    let file = folder.join("b.png");
    // The first open fails b. The second open holds a, so b waits with the
    // remembered failure in its job.
    let (render, gate) = scripted(&[Step::Ok, Step::Fail, Step::HoldThenOk]);
    let mut setup = start(dir, 1, render);
    setup.pipeline.open_folder(&folder).expect("open");
    collect(&setup.results, 2);
    // A new size makes a render again, whatever the writer has stored.
    RgbaImage::new(50, 50)
        .save(folder.join("a.png"))
        .expect("rewrite a");
    setup.pipeline.open_folder(&folder).expect("open again");
    let held = gate.entered.recv_timeout(WAIT);

    let queued = setup.pipeline.retry(&HashSet::from([file.clone()]));
    gate.release.send(()).expect("release");
    held.expect("the render of a started");
    assert_eq!(queued, 1);

    let results = collect(&setup.results, 2);
    let b = results
        .iter()
        .find(|r| r.key == file)
        .expect("a result for b");
    assert!(b.outcome.is_ok(), "b rendered instead of the old failure");
    assert!(
        setup
            .results
            .recv_timeout(Duration::from_millis(200))
            .is_none(),
        "no second result for b"
    );
}

#[test]
fn a_failure_of_a_render_that_began_before_the_retry_is_not_remembered() {
    let dir = TempDir::new();
    let folder = folder_with(&dir, &["b.png"]);
    let file = folder.join("b.png");
    // The first render is held and then fails. The retry renders on the other
    // worker and works before the held render ends.
    let (render, gate) = scripted(&[Step::HoldThenFail]);
    let mut setup = start(dir, 2, render);
    setup.pipeline.open_folder(&folder).expect("open");
    let held = gate.entered.recv_timeout(WAIT);

    let queued = setup.pipeline.retry(&HashSet::from([file.clone()]));
    let retried = setup.results.recv_timeout(WAIT);
    gate.release.send(()).expect("release");
    held.expect("the first render started");
    assert_eq!(queued, 1);
    assert!(retried.expect("the retry result").outcome.is_ok());
    let old = collect(&setup.results, 1);
    assert!(old[0].outcome.is_err(), "the held render failed");

    assert!(
        !setup
            .pipeline
            .failures
            .lock()
            .expect("lock")
            .contains_key(&file),
        "the old failure is not remembered"
    );
    setup.pipeline.flush();
    setup.pipeline.open_folder(&folder).expect("open again");
    assert!(collect(&setup.results, 1)[0].outcome.is_ok());
}

#[test]
fn a_render_that_works_clears_a_failure_remembered_meanwhile() {
    let dir = TempDir::new();
    let folder = folder_with(&dir, &["b.png"]);
    let file = folder.join("b.png");
    // The open fails. The first retry is held and fails, so it is remembered.
    // The second retry works and must clear that entry.
    let (render, gate) = scripted(&[Step::Fail, Step::HoldThenFail]);
    let mut setup = start(dir, 1, render);
    setup.pipeline.open_folder(&folder).expect("open");
    assert!(collect(&setup.results, 1)[0].outcome.is_err());
    let keys = HashSet::from([file.clone()]);
    setup.pipeline.retry(&keys);
    let held = gate.entered.recv_timeout(WAIT);
    setup.pipeline.retry(&keys);
    gate.release.send(()).expect("release");
    held.expect("the first retry started");

    let results = collect(&setup.results, 2);
    assert!(results[0].outcome.is_err());
    assert!(results[1].outcome.is_ok());
    assert!(
        !setup
            .pipeline
            .failures
            .lock()
            .expect("lock")
            .contains_key(&file),
        "the working render cleared the failure"
    );
    setup.pipeline.flush();
    setup.pipeline.open_folder(&folder).expect("open again");
    assert!(collect(&setup.results, 1)[0].outcome.is_ok());
}
