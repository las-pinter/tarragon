//! Lock, start, forward, race, and shutdown tests.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Barrier};
use std::time::Duration;

use super::super::tcp::TcpTransport;
use super::super::*;
use super::{TempDir, primary};

/// A second start is forwarded, and the primary sees both the flag and the waker call.
fn first_primary_then_forwarded<T: Transport>() {
    let dir = TempDir::new();
    let layout = dir.layout();
    let guard = primary(acquire_with::<T>(&layout));
    let woken = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&woken);
    guard.set_waker(move || {
        counter.fetch_add(1, Ordering::SeqCst);
    });
    assert!(!guard.take_activate_request());

    assert!(matches!(acquire_with::<T>(&layout), Outcome::Forwarded));
    assert!(guard.take_activate_request());
    assert!(!guard.take_activate_request(), "the flag is taken once");
    assert_eq!(woken.load(Ordering::SeqCst), 1);
}

#[cfg(unix)]
#[test]
fn unix_second_start_is_forwarded() {
    first_primary_then_forwarded::<unix::UnixTransport>();
}

#[test]
fn tcp_second_start_is_forwarded() {
    first_primary_then_forwarded::<TcpTransport>();
}

#[cfg(unix)]
#[test]
fn stale_socket_without_a_lock_holder_is_removed() {
    let dir = TempDir::new();
    let layout = dir.layout();
    drop(std::os::unix::net::UnixListener::bind(&layout.sock).expect("bind"));
    assert!(
        layout.sock.exists(),
        "dropping the listener leaves the file"
    );

    let guard = primary(acquire_with::<unix::UnixTransport>(&layout));
    assert!(matches!(
        acquire_with::<unix::UnixTransport>(&layout),
        Outcome::Forwarded
    ));
    assert!(guard.take_activate_request());
}

#[test]
fn stale_port_file_is_replaced() {
    let dir = TempDir::new();
    let layout = dir.layout();
    std::fs::write(&layout.port, proto::format_port_file(9, "old")).expect("write");
    let guard = primary(acquire_with::<TcpTransport>(&layout));
    let text = std::fs::read_to_string(&layout.port).expect("read");
    let (port, token) = proto::parse_port_file(&text).expect("parse");
    assert_ne!(port, 9);
    assert_ne!(token, "old");
    drop(guard);
}

/// Two handles of one process exclude each other on the lock file.
#[test]
fn file_lock_excludes_a_second_handle_in_the_same_process() {
    let dir = TempDir::new();
    let layout = dir.layout();
    let first = open_lock(&layout.lock).expect("open");
    let second = open_lock(&layout.lock).expect("open");
    first.try_lock().expect("first lock");
    assert!(matches!(second.try_lock(), Err(TryLockError::WouldBlock)));
    drop(first);
    second.try_lock().expect("lock after release");
}

/// Eight starts at once make one Primary and seven forwards.
fn race<T: Transport>() {
    let dir = TempDir::new();
    let layout = Arc::new(dir.layout());
    let barrier = Arc::new(Barrier::new(8));
    let handles: Vec<_> = (0..8)
        .map(|_| {
            let layout = Arc::clone(&layout);
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                barrier.wait();
                acquire_with::<T>(&layout)
            })
        })
        .collect();
    let outcomes: Vec<_> = handles
        .into_iter()
        .map(|handle| handle.join().expect("thread"))
        .collect();
    let primaries: Vec<_> = outcomes
        .iter()
        .filter(|o| matches!(o, Outcome::Primary(_)))
        .collect();
    assert_eq!(primaries.len(), 1);
    let forwarded = outcomes
        .iter()
        .filter(|o| matches!(o, Outcome::Forwarded))
        .count();
    assert_eq!(forwarded, 7);
    let Some(Outcome::Primary(guard)) = outcomes.iter().find(|o| matches!(o, Outcome::Primary(_)))
    else {
        unreachable!("one primary was counted");
    };
    assert!(guard.take_activate_request());
}

#[cfg(unix)]
#[test]
fn unix_eight_racing_starts_make_one_primary() {
    race::<unix::UnixTransport>();
}

#[test]
fn tcp_eight_racing_starts_make_one_primary() {
    race::<TcpTransport>();
}

/// After drop nothing listens, the files are gone, and the lock is free.
fn listener_stops_on_drop<T: Transport>() {
    let dir = TempDir::new();
    let layout = dir.layout();
    let guard = primary(acquire_with::<T>(&layout));
    drop(guard);
    assert!(T::connect(&layout).is_err(), "nothing listens after drop");
    for file in T::files(&layout) {
        assert!(!file.exists(), "{} is gone", file.display());
    }
    assert!(layout.lock.exists(), "the lock file stays");
    // The lock is free again.
    drop(primary(acquire_with::<T>(&layout)));
}

#[cfg(unix)]
#[test]
fn unix_listener_stops_on_drop() {
    listener_stops_on_drop::<unix::UnixTransport>();
}

#[test]
fn tcp_listener_stops_on_drop() {
    listener_stops_on_drop::<TcpTransport>();
}

#[test]
fn unreadable_lock_path_gives_unguarded() {
    let dir = TempDir::new();
    let mut layout = dir.layout();
    layout.lock = dir.0.join("missing").join("x.lock");
    assert!(matches!(
        acquire_with::<TcpTransport>(&layout),
        Outcome::Unguarded(_)
    ));
}

#[test]
fn data_root_is_the_parent_of_the_db_file() {
    let dir = TempDir::new();
    let root = data_root(&dir.0.join("a.db"));
    assert_eq!(root, dir.0.canonicalize().expect("canonical"));
    assert_eq!(data_root(&dir.0.join("b.db")), root);
    assert!(data_root(Path::new("x.db")).is_absolute());
}

/// A bind that fails gives Unguarded and frees the lock.
fn bind_failure_is_unguarded_and_frees_the_lock<T: Transport>() {
    let dir = TempDir::new();
    let good = dir.layout();
    let mut bad = good.clone();
    bad.sock = dir.0.join("missing").join("x.sock");
    bad.port = dir.0.join("missing").join("x.port");
    assert!(matches!(acquire_with::<T>(&bad), Outcome::Unguarded(_)));
    drop(primary(acquire_with::<T>(&good)));
}

#[cfg(unix)]
#[test]
fn unix_bind_failure_is_unguarded_and_frees_the_lock() {
    bind_failure_is_unguarded_and_frees_the_lock::<unix::UnixTransport>();
}

#[test]
fn tcp_bind_failure_is_unguarded_and_frees_the_lock() {
    bind_failure_is_unguarded_and_frees_the_lock::<TcpTransport>();
}

#[cfg(unix)]
#[test]
fn over_long_socket_path_is_unguarded() {
    let long = std::env::temp_dir().join("d".repeat(120));
    let db = std::env::temp_dir().join("x.db");
    let outcome = acquire_in(
        &db,
        Some(long.into_os_string()),
        &std::env::temp_dir(),
        |_| Owner {
            name: None,
            uid: None,
        },
    );
    assert!(matches!(outcome, Outcome::Unguarded(reason) if reason.contains("too long")));
}

/// A TCP transport whose first connect drops a lock holder and fails, to
/// model a primary that quits right after the first failed try. Every other
/// call goes to the real transport.
struct QuitsOnConnect(TcpTransport);

thread_local! {
    /// The lock holder to drop on the first connect.
    static HOLDER: std::cell::RefCell<Option<File>> = const { std::cell::RefCell::new(None) };
}

impl Transport for QuitsOnConnect {
    type Stream = std::net::TcpStream;

    fn bind(layout: &Layout) -> std::io::Result<Self> {
        TcpTransport::bind(layout).map(Self)
    }

    fn token(&self) -> Option<&str> {
        self.0.token()
    }

    fn accept(&self) -> std::io::Result<Self::Stream> {
        self.0.accept()
    }

    fn set_read_timeout(stream: &Self::Stream, timeout: Duration) -> std::io::Result<()> {
        TcpTransport::set_read_timeout(stream, timeout)
    }

    fn waker(&self) -> transport::WakeFn {
        self.0.waker()
    }

    fn connect(layout: &Layout) -> std::io::Result<(Self::Stream, Option<String>)> {
        if HOLDER.with(|holder| holder.borrow_mut().take()).is_some() {
            return Err(std::io::ErrorKind::ConnectionRefused.into());
        }
        TcpTransport::connect(layout)
    }

    fn files(layout: &Layout) -> Vec<PathBuf> {
        TcpTransport::files(layout)
    }
}

#[test]
fn forward_takes_the_lock_when_the_holder_quits_after_a_failed_try() {
    let dir = TempDir::new();
    let layout = dir.layout();
    let holder = open_lock(&layout.lock).expect("open");
    holder.try_lock().expect("lock");
    HOLDER.with(|slot| *slot.borrow_mut() = Some(holder));
    let ours = open_lock(&layout.lock).expect("open");
    assert!(matches!(
        transport::forward::<QuitsOnConnect>(&layout, &ours),
        Forwarded::TookLock
    ));
}

#[test]
fn forward_fails_when_the_holder_stays_and_nothing_answers() {
    let dir = TempDir::new();
    let layout = dir.layout();
    let holder = open_lock(&layout.lock).expect("open");
    holder.try_lock().expect("lock");
    let ours = open_lock(&layout.lock).expect("open");
    // No holder is stored for the connect to drop, so it keeps the lock.
    assert!(matches!(
        transport::forward::<QuitsOnConnect>(&layout, &ours),
        Forwarded::Failed(_)
    ));
    drop(holder);
}

#[test]
fn start_after_taking_the_lock_gives_a_working_primary() {
    let dir = TempDir::new();
    let layout = dir.layout();
    let holder = open_lock(&layout.lock).expect("open");
    holder.try_lock().expect("lock");
    HOLDER.with(|slot| *slot.borrow_mut() = Some(holder));

    let guard = primary(acquire_with::<QuitsOnConnect>(&layout));
    assert!(!guard.take_activate_request());
    assert!(matches!(
        acquire_with::<QuitsOnConnect>(&layout),
        Outcome::Forwarded
    ));
    assert!(guard.take_activate_request());
}
