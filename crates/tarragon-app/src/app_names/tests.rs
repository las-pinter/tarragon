use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

use super::*;

/// How long a test waits for the worker.
const WAIT: Duration = Duration::from_secs(5);

/// Poll until `done` holds or the wait ends.
fn poll_until(names: &mut AppNames, mut done: impl FnMut(&mut AppNames) -> bool) {
    let start = Instant::now();
    while !done(names) {
        assert!(start.elapsed() < WAIT, "the worker did not answer");
        names.poll();
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn a_name_arrives_once_per_extension_and_wakes_the_window() {
    let calls = Arc::new(AtomicUsize::new(0));
    let woken = Arc::new(AtomicUsize::new(0));
    let (counted, wake) = (Arc::clone(&calls), Arc::clone(&woken));
    let repaint: Repaint = Arc::new(move || {
        wake.fetch_add(1, Ordering::SeqCst);
    });
    let mut names = AppNames::with_lookup(repaint, move |_: &Path| {
        counted.fetch_add(1, Ordering::SeqCst);
        Ok::<_, ()>(Some("GIMP".to_owned()))
    });
    assert_eq!(names.name_for(Path::new("/a/x.png")), None);
    poll_until(&mut names, |n| n.name_for(Path::new("/b/y.PNG")).is_some());
    assert_eq!(names.name_for(Path::new("/c/z.png")), Some("GIMP"));
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "one lookup for the extension"
    );
    assert!(woken.load(Ordering::SeqCst) >= 1);
}

#[test]
fn a_timeout_stops_every_later_lookup() {
    let (seen, files) = mpsc::channel::<PathBuf>();
    let repaint: Repaint = Arc::new(|| {});
    let mut names = AppNames::with_lookup(repaint, move |file: &Path| {
        let _ = seen.send(file.to_path_buf());
        Err::<Option<String>, ()>(())
    });
    assert_eq!(names.name_for(Path::new("/a/x.png")), None);
    poll_until(&mut names, |n| n.stopped);
    assert_eq!(names.name_for(Path::new("/a/x.psd")), None);
    assert_eq!(names.name_for(Path::new("/a/x.png")), None);
    std::thread::sleep(Duration::from_millis(50));
    assert_eq!(files.try_iter().count(), 1, "no lookup after the timeout");
}
