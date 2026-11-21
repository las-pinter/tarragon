use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::*;

/// A clipboard in memory. A copy of no files fails like the system one.
#[derive(Default)]
struct Memory {
    files: Arc<Mutex<Vec<PathBuf>>>,
}

impl Backend for Memory {
    fn set_files(&mut self, paths: &[PathBuf]) -> Result<(), ClipboardError> {
        if paths.is_empty() {
            return Err(ClipboardError::NothingToCopy);
        }
        *self.files.lock().expect("lock") = paths.to_vec();
        Ok(())
    }

    fn files(&mut self) -> Result<Vec<PathBuf>, ClipboardError> {
        Ok(self.files.lock().expect("lock").clone())
    }
}

/// A clipboard that always gives `error`, after `delay`.
struct Failing {
    error: ClipboardError,
    delay: Duration,
}

impl Backend for Failing {
    fn set_files(&mut self, _paths: &[PathBuf]) -> Result<(), ClipboardError> {
        std::thread::sleep(self.delay);
        Err(self.error.clone())
    }

    fn files(&mut self) -> Result<Vec<PathBuf>, ClipboardError> {
        std::thread::sleep(self.delay);
        Err(self.error.clone())
    }
}

fn copy(service: &Service, paths: &[PathBuf], timeout: Duration) -> Result<(), ClipboardError> {
    let paths = paths.to_vec();
    send_and_wait(
        &service.requests(),
        |answer| Request::Copy(paths, answer),
        timeout,
    )
}

fn read(service: &Service, timeout: Duration) -> Result<Vec<PathBuf>, ClipboardError> {
    send_and_wait(&service.requests(), Request::Read, timeout)
}

const WAIT: Duration = Duration::from_secs(5);

#[test]
fn a_copied_list_reads_back_through_the_thread() {
    let service = Service::start(Box::new(|| {
        Ok(Box::new(Memory::default()) as Box<dyn Backend>)
    }))
    .expect("start");
    let paths = vec![PathBuf::from("/pics/a.png"), PathBuf::from("/pics/b c.psd")];
    copy(&service, &paths, WAIT).expect("copy");
    assert_eq!(read(&service, WAIT).expect("read"), paths);
    service.stop(WAIT);
}

#[test]
fn the_thread_keeps_one_backend_for_every_request() {
    let made = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&made);
    let service = Service::start(Box::new(move || {
        counter.fetch_add(1, Ordering::SeqCst);
        Ok(Box::new(Memory::default()) as Box<dyn Backend>)
    }))
    .expect("start");
    copy(&service, &[PathBuf::from("/a.png")], WAIT).expect("copy");
    read(&service, WAIT).expect("read");
    read(&service, WAIT).expect("read");
    assert_eq!(made.load(Ordering::SeqCst), 1);
    service.stop(WAIT);
}

#[test]
fn a_clipboard_that_cannot_open_is_unavailable_and_tried_again_later() {
    let tries = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&tries);
    let service = Service::start(Box::new(move || {
        if counter.fetch_add(1, Ordering::SeqCst) == 0 {
            return Err(ClipboardError::Unavailable("no display".into()));
        }
        Ok(Box::new(Memory::default()) as Box<dyn Backend>)
    }))
    .expect("start");
    assert_eq!(
        read(&service, WAIT),
        Err(ClipboardError::Unavailable("no display".into()))
    );
    assert_eq!(read(&service, WAIT), Ok(Vec::new()));
    assert_eq!(tries.load(Ordering::SeqCst), 2);
    service.stop(WAIT);
}

#[test]
fn a_backend_error_reaches_the_caller() {
    let service = Service::start(Box::new(|| {
        Ok(Box::new(Failing {
            error: ClipboardError::Busy,
            delay: Duration::ZERO,
        }) as Box<dyn Backend>)
    }))
    .expect("start");
    assert_eq!(
        copy(&service, &[PathBuf::from("/a.png")], WAIT),
        Err(ClipboardError::Busy)
    );
    assert_eq!(read(&service, WAIT), Err(ClipboardError::Busy));
    service.stop(WAIT);
}

#[test]
fn a_stalled_clipboard_times_out_without_holding_the_caller() {
    let service = Service::start(Box::new(|| {
        Ok(Box::new(Failing {
            error: ClipboardError::Busy,
            delay: Duration::from_millis(500),
        }) as Box<dyn Backend>)
    }))
    .expect("start");
    let start = std::time::Instant::now();
    assert_eq!(
        read(&service, Duration::from_millis(50)),
        Err(ClipboardError::Timeout)
    );
    assert!(start.elapsed() < Duration::from_millis(400));
    service.stop(WAIT);
}

#[test]
fn stop_drops_the_backend_and_ends_the_thread() {
    struct Dropped(Arc<AtomicUsize>);
    impl Backend for Dropped {
        fn set_files(&mut self, _paths: &[PathBuf]) -> Result<(), ClipboardError> {
            Ok(())
        }
        fn files(&mut self) -> Result<Vec<PathBuf>, ClipboardError> {
            Ok(Vec::new())
        }
    }
    impl Drop for Dropped {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }
    let drops = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&drops);
    let service = Service::start(Box::new(move || {
        Ok(Box::new(Dropped(Arc::clone(&counter))) as Box<dyn Backend>)
    }))
    .expect("start");
    read(&service, WAIT).expect("read");
    assert_eq!(drops.load(Ordering::SeqCst), 0);
    service.stop(WAIT);
    assert_eq!(drops.load(Ordering::SeqCst), 1);
}

#[cfg(unix)]
#[test]
fn pasted_entries_keep_absolute_and_localhost_paths_only() {
    let listed = [
        "/a.png",
        "localhost/pics/b.png",
        "otherhost/c.png",
        "rel.png",
    ]
    .map(PathBuf::from)
    .to_vec();
    assert_eq!(
        local_paths(listed),
        [PathBuf::from("/a.png"), PathBuf::from("/pics/b.png")]
    );
}

#[test]
fn arboard_errors_map_to_the_app_errors() {
    assert_eq!(
        map_error(arboard::Error::ClipboardOccupied),
        ClipboardError::Busy
    );
    assert!(matches!(
        map_error(arboard::Error::ClipboardNotSupported),
        ClipboardError::Unavailable(_)
    ));
    assert!(matches!(
        map_error(arboard::Error::Unknown {
            description: "x".into()
        }),
        ClipboardError::Failed(_)
    ));
}

/// A temp folder that removes itself on drop.
#[cfg(any(windows, target_os = "macos", target_os = "linux"))]
struct TempDir(PathBuf);

#[cfg(any(windows, target_os = "macos", target_os = "linux"))]
impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Copy two real files to the system clipboard and read them back. It runs
/// on the Windows and macOS test jobs. On Linux it would use the display of
/// the shell, which may be the user's screen, so it is ignored there and the
/// Linux CI job runs it alone under `xvfb-run`.
#[cfg(any(windows, target_os = "macos", target_os = "linux"))]
#[cfg_attr(
    target_os = "linux",
    ignore = "needs a display; CI runs it under xvfb-run"
)]
#[test]
fn the_system_clipboard_round_trips_a_file_list() {
    let dir = TempDir(std::env::temp_dir().join(format!(
        "tarragon-clipboard-{}",
        tarragon_core::testing::run_tag()
    )));
    std::fs::create_dir_all(&dir.0).expect("make the folder");
    let paths = vec![dir.0.join("a.png"), dir.0.join("b c#1.psd")];
    for path in &paths {
        std::fs::write(path, b"x").expect("write a file");
    }

    copy_files(&paths).expect("copy to the system clipboard");
    let read = read_files().expect("read the system clipboard");

    let canonical = |list: &[PathBuf]| {
        let mut out: Vec<PathBuf> = list
            .iter()
            .map(|path| dunce::canonicalize(path).expect("canonicalize"))
            .collect();
        out.sort();
        out
    };
    assert_eq!(canonical(&read), canonical(&paths));
}
