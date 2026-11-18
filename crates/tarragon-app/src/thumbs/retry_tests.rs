//! Tests for the failure lookup and the retry of a failed file.

use std::sync::atomic::{AtomicUsize, Ordering};

use image::DynamicImage;
use tarragon_core::error::RenderError;

use super::tests::Rig;
use super::*;

/// A render function whose first call fails and whose later calls work. Every
/// call is counted.
fn failing_once() -> (crate::pipeline::RenderFn, Arc<AtomicUsize>) {
    let calls = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&calls);
    let render: crate::pipeline::RenderFn = Arc::new(move |path| {
        if counter.fetch_add(1, Ordering::SeqCst) == 0 {
            return Err(RenderError::Decode {
                path: path.to_path_buf(),
                reason: "broken on purpose".to_owned(),
            });
        }
        Ok(DynamicImage::ImageRgba8(RgbaImage::new(8, 8)))
    });
    (render, calls)
}

#[test]
fn a_failed_file_gives_its_error_and_a_retry_clears_it() {
    let (render, calls) = failing_once();
    let mut rig = Rig::with_render("retry-idle", None, render);
    let folder = rig.folder("one", &["a.png"]);
    let file = folder.join("a.png");
    rig.thumbs.navigate(folder);
    assert!(rig.settle().starts_with("1 tiles settled, 1 failed"));
    let error = rig.thumbs.failure(&file).expect("the failure is known");
    assert!(error.message.contains("broken on purpose"), "{error:?}");

    rig.thumbs.retry(&HashSet::from([file.clone()]));
    let start = Instant::now();
    while rig.thumbs.failure(&file).is_some() {
        rig.thumbs.before_draw(&rig.ctx, &mut rig.gallery);
        assert!(start.elapsed() < Duration::from_secs(30), "no retry result");
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[test]
fn a_retry_while_the_open_runs_queues_nothing() {
    let calls = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&calls);
    let render: crate::pipeline::RenderFn = Arc::new(move |_| {
        counter.fetch_add(1, Ordering::SeqCst);
        Ok(DynamicImage::ImageRgba8(RgbaImage::new(8, 8)))
    });
    let mut rig = Rig::with_render("retry-opening", None, render);
    let folder = rig.folder("one", &["a.png"]);
    let file = folder.join("a.png");
    rig.thumbs.navigate(folder);
    // The first frame moves the pipeline into the open thread.
    rig.thumbs.before_draw(&rig.ctx, &mut rig.gallery);
    assert!(matches!(rig.thumbs.slot, Slot::Opening { .. }));

    rig.thumbs.retry(&HashSet::from([file]));
    assert!(matches!(rig.thumbs.slot, Slot::Opening { .. }));
    assert!(rig.settle().starts_with("1 tiles settled, 0 failed"));
    // A queued retry would render a second time.
    std::thread::sleep(Duration::from_millis(200));
    rig.thumbs.before_draw(&rig.ctx, &mut rig.gallery);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}
