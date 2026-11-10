//! Tests for the log line of a failed render.

use tarragon_core::error::RenderError;
use tracing::Level;
use tracing::subscriber::DefaultGuard;
use tracing_subscriber::layer::SubscriberExt;

use super::*;
use crate::logging::ring::{LogRing, RingLayer};

/// Keep the log lines of this thread and of the pipelines it starts. Start the
/// pipeline after this call, because a worker takes the subscriber at its start.
fn capture() -> (LogRing, DefaultGuard) {
    let ring = LogRing::new();
    let layer = RingLayer::new(ring.clone(), time::UtcOffset::UTC);
    let guard = tracing::subscriber::set_default(tracing_subscriber::registry().with(layer));
    (ring, guard)
}

/// The text of every ERROR line.
fn errors(ring: &LogRing) -> Vec<String> {
    ring.snapshot()
        .into_iter()
        .filter(|entry| entry.level == Level::ERROR)
        .map(|entry| entry.text)
        .collect()
}

/// A render function that fails with `fail` on every call.
fn always_failing(fail: fn(&Path) -> RenderError) -> RenderFn {
    Arc::new(move |path| Err(fail(path)))
}

fn decode_error(path: &Path) -> RenderError {
    RenderError::Decode {
        path: path.to_path_buf(),
        reason: "broken on purpose".to_owned(),
    }
}

fn cancelled(path: &Path) -> RenderError {
    RenderError::Cancelled {
        path: path.to_path_buf(),
    }
}

/// A one-file folder and its file.
fn one_file(dir: &TempDir) -> (PathBuf, PathBuf) {
    let folder = dir.sub("log");
    let file = folder.join("one.png");
    write_png(&file, 255);
    (folder, file)
}

#[test]
fn a_decode_failure_logs_one_error_with_path_and_text() {
    let (ring, _guard) = capture();
    let dir = TempDir::new();
    let (folder, file) = one_file(&dir);
    let mut setup = start(dir, 1, always_failing(decode_error));

    setup.pipeline.open_folder(&folder).expect("open");
    assert!(collect(&setup.results, 1)[0].outcome.is_err());

    let lines = errors(&ring);
    assert_eq!(lines.len(), 1, "one line: {lines:?}");
    assert!(lines[0].contains("cannot render "), "{}", lines[0]);
    assert!(
        lines[0].contains(&file.display().to_string()),
        "{}",
        lines[0]
    );
    assert!(lines[0].contains("broken on purpose"), "{}", lines[0]);
}

#[test]
fn a_stored_failure_adds_no_line() {
    let (ring, _guard) = capture();
    let dir = TempDir::new();
    let (folder, file) = one_file(&dir);
    let mut setup = start(dir, 1, always_failing(decode_error));
    setup.pipeline.open_folder(&folder).expect("open");
    collect(&setup.results, 1);

    // The same file with the same stamp is answered from the failure list.
    setup.pipeline.open_folder(&folder).expect("open again");
    assert!(collect(&setup.results, 1)[0].outcome.is_err());

    assert!(
        setup
            .pipeline
            .jobs
            .get(&file)
            .is_some_and(|job| job.known_failure.is_some()),
        "the second job used the stored failure"
    );
    assert_eq!(errors(&ring).len(), 1);
}

#[test]
fn a_retry_that_fails_again_adds_one_line() {
    let (ring, _guard) = capture();
    let dir = TempDir::new();
    let (folder, file) = one_file(&dir);
    let mut setup = start(dir, 1, always_failing(decode_error));
    setup.pipeline.open_folder(&folder).expect("open");
    collect(&setup.results, 1);

    assert_eq!(setup.pipeline.retry(&HashSet::from([file])), 1);
    assert!(collect(&setup.results, 1)[0].outcome.is_err());

    assert_eq!(errors(&ring).len(), 2);
}

#[test]
fn a_render_that_works_or_hits_the_cache_logs_no_error() {
    let (ring, _guard) = capture();
    let dir = TempDir::new();
    let (folder, file) = one_file(&dir);
    let calls = Arc::default();
    let mut setup = start(dir, 1, counting_render(&calls));
    setup.pipeline.open_folder(&folder).expect("open");
    assert!(collect(&setup.results, 1)[0].outcome.is_ok());
    setup.pipeline.flush();

    setup.pipeline.request(&HashSet::from([file]));
    assert!(collect(&setup.results, 1)[0].outcome.is_ok());

    assert_eq!(calls.lock().expect("lock").len(), 1, "the second was a hit");
    assert_eq!(errors(&ring), Vec::<String>::new());
}

#[test]
fn a_cancelled_render_logs_no_error() {
    let (ring, _guard) = capture();
    let dir = TempDir::new();
    let (folder, _) = one_file(&dir);
    let mut setup = start(dir, 1, always_failing(cancelled));

    setup.pipeline.open_folder(&folder).expect("open");
    let error = collect(&setup.results, 1)
        .remove(0)
        .outcome
        .expect_err("the render fails");

    assert_eq!(error.kind, ErrorKind::Cancelled);
    assert_eq!(errors(&ring), Vec::<String>::new());
}
