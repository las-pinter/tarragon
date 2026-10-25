//! The single-instance guard.
//!
//! The first start takes an OS file lock and listens for a one-line
//! "activate" request. A later start finds the lock held, sends that line,
//! and exits. The OS drops the lock when its process dies, so a crashed
//! primary never blocks the next start. The guard is best effort: any
//! problem with it means the app starts without one.

mod dir;
mod paths;
mod proto;
#[cfg(any(not(unix), test))]
mod tcp;
mod transport;
#[cfg(unix)]
mod unix;

#[cfg(test)]
mod tests;

use std::fs::{File, OpenOptions, TryLockError};
use std::panic::AssertUnwindSafe;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::JoinHandle;

use paths::{Layout, Location};
use transport::{Forwarded, Transport, WakeFn};

#[cfg(not(unix))]
type Platform = tcp::TcpTransport;
#[cfg(unix)]
type Platform = unix::UnixTransport;

/// A function the listener calls after it sets the activate flag.
type Waker = Arc<dyn Fn() + Send + Sync>;

/// What a start found.
pub enum Outcome {
    /// This process is the first one. Keep the guard until the end.
    Primary(Guard),
    /// Another instance runs. It was asked to show its window, or the ask
    /// failed and was logged.
    Forwarded,
    /// The guard could not be set up. The app runs without one.
    Unguarded(String),
}

/// The state shared with the listener thread.
#[derive(Clone, Default)]
struct Delivery {
    /// Set when a second start asked for the window.
    flag: Arc<AtomicBool>,
    /// Called after the flag is set.
    waker: Arc<Mutex<Option<Waker>>>,
}

impl Delivery {
    /// Record a request and wake the UI.
    fn signal(&self) {
        self.flag.store(true, Ordering::Release);
        // Clone it out so no lock is held while it runs: it may call
        // `set_waker`. A panic in it must not end the listener thread.
        let waker = self
            .waker
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        if let Some(wake) = waker
            && std::panic::catch_unwind(AssertUnwindSafe(|| wake())).is_err()
        {
            tracing::error!("the activate waker panicked");
        }
    }
}

/// Proof that this process is the primary instance. Dropping it stops the
/// listener and removes the socket. The lock file stays, because deleting
/// it would open a race between two starts.
pub struct Guard {
    /// Where requests are recorded.
    delivery: Delivery,
    /// Tells the listener to end.
    stop: Arc<AtomicBool>,
    /// Ends the blocked accept of the listener.
    wake: WakeFn,
    /// The listener thread.
    thread: Option<JoinHandle<()>>,
    /// The files to remove on drop.
    files: Vec<PathBuf>,
    /// The held lock. It is released when the guard is gone.
    _lock: File,
}

impl Guard {
    /// Whether a second start asked for the window since the last call.
    ///
    /// Poll this each frame and raise the window. On Wayland a focus request
    /// has no effect, so also request user attention.
    #[cfg_attr(not(test), expect(dead_code, reason = "read by the UI loop"))]
    pub fn take_activate_request(&self) -> bool {
        self.delivery.flag.swap(false, Ordering::AcqRel)
    }

    /// Set the function the listener calls after it records a request, for
    /// example one that asks the UI for a repaint. It runs on the listener
    /// thread. A later call replaces the earlier function.
    ///
    /// The function must not own the guard. If the last guard dropped on the
    /// listener thread, that thread would join itself.
    #[cfg_attr(not(test), expect(dead_code, reason = "set by the UI loop"))]
    pub fn set_waker(&self, waker: impl Fn() + Send + Sync + 'static) {
        *self
            .delivery
            .waker
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(Arc::new(waker));
    }
}

impl Drop for Guard {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        // Join only when the wake-up connect worked, so a failure cannot
        // hang the exit. A request is read within a fixed deadline, so the
        // listener is never busy for long.
        if (self.wake)()
            && let Some(thread) = self.thread.take()
        {
            let _ = thread.join();
        }
        // The files go before the lock is released, so a new primary never
        // loses its own socket to this cleanup.
        for file in &self.files {
            let _ = std::fs::remove_file(file);
        }
    }
}

/// Take the guard for the instance that owns `db_path`.
///
/// Two starts share an instance when their database files share a folder.
pub fn acquire(db_path: &Path) -> Outcome {
    acquire_in(
        db_path,
        std::env::var_os("XDG_RUNTIME_DIR"),
        &std::env::temp_dir(),
        current_owner,
    )
}

/// Who runs this process. It names the private folder and is checked as the
/// folder owner.
struct Owner {
    /// Keeps users apart in the temp folder.
    name: Option<std::ffi::OsString>,
    /// The user id, where the folder owner is checked.
    uid: Option<u32>,
}

/// Find the owner. On unix the id is the owner of a file we create, which the
/// environment cannot fake. The probe runs once per start.
#[cfg(unix)]
fn current_owner(temp: &Path) -> Owner {
    let uid = dir::own_uid(temp).ok();
    Owner {
        name: uid.map(|uid| uid.to_string().into()),
        uid,
    }
}

/// Find the owner by the user name.
#[cfg(not(unix))]
fn current_owner(_temp: &Path) -> Owner {
    Owner {
        name: std::env::var_os("USERNAME"),
        uid: None,
    }
}

/// [`acquire`] with the environment values passed in. `owner` runs only when
/// the temp folder is used.
fn acquire_in(
    db_path: &Path,
    xdg_runtime_dir: Option<std::ffi::OsString>,
    temp: &Path,
    owner: impl FnOnce(&Path) -> Owner,
) -> Outcome {
    let mut found = None;
    let location = paths::locate(xdg_runtime_dir, temp, || {
        let owner = owner(temp);
        let name = owner.name.clone();
        found = Some(owner);
        name
    });
    let uid = found.and_then(|owner| owner.uid);
    let layout = Layout::new(&location, &paths::key(&data_root(db_path)));
    if cfg!(unix) && !layout.socket_path_fits() {
        return Outcome::Unguarded(format!(
            "the socket path is too long: {}",
            layout.sock.display()
        ));
    }
    prepare_and_acquire::<Platform>(&location, uid, &layout)
}

/// The folder of the database file, as a canonical path when possible.
fn data_root(db_path: &Path) -> PathBuf {
    let parent = db_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    parent
        .canonicalize()
        .unwrap_or_else(|_| parent.to_path_buf())
}

/// Check the folder, then take the guard.
fn prepare_and_acquire<T: Transport>(
    location: &Location,
    uid: Option<u32>,
    layout: &Layout,
) -> Outcome {
    if let Err(error) = dir::prepare(location, uid) {
        return Outcome::Unguarded(format!("cannot use {}: {error}", location.dir().display()));
    }
    acquire_with::<T>(layout)
}

/// Open the lock file, creating it when it is missing.
fn open_lock(path: &Path) -> std::io::Result<File> {
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    options.open(path)
}

/// Take the lock. The holder becomes the primary. Any other process sends
/// the activate request to the primary.
fn acquire_with<T: Transport>(layout: &Layout) -> Outcome {
    let lock = match open_lock(&layout.lock) {
        Ok(lock) => lock,
        Err(error) => {
            return Outcome::Unguarded(format!(
                "cannot open the lock file {}: {error}",
                layout.lock.display()
            ));
        }
    };
    match lock.try_lock() {
        Ok(()) => start::<T>(layout, lock),
        Err(TryLockError::WouldBlock) => match transport::forward::<T>(layout, &lock) {
            Forwarded::Sent => Outcome::Forwarded,
            Forwarded::TookLock => start::<T>(layout, lock),
            Forwarded::Failed(error) => {
                tracing::warn!("cannot ask the running instance to show its window: {error}");
                Outcome::Forwarded
            }
        },
        Err(TryLockError::Error(error)) => {
            Outcome::Unguarded(format!("cannot lock {}: {error}", layout.lock.display()))
        }
    }
}

/// Clear what a dead primary left, bind, and start the listener thread.
fn start<T: Transport>(layout: &Layout, lock: File) -> Outcome {
    let files = T::files(layout);
    // We hold the lock, so no live process owns these files.
    for file in &files {
        match std::fs::remove_file(file) {
            Ok(()) => tracing::info!("removed the stale file {}", file.display()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => tracing::warn!("cannot remove {}: {error}", file.display()),
        }
    }
    let transport = match T::bind(layout) {
        Ok(transport) => transport,
        Err(error) => return Outcome::Unguarded(format!("cannot listen: {error}")),
    };
    let wake = transport.waker();
    let delivery = Delivery::default();
    let stop = Arc::new(AtomicBool::new(false));
    let spawned = std::thread::Builder::new()
        .name("instance-listener".into())
        .spawn({
            let delivery = delivery.clone();
            let stop = Arc::clone(&stop);
            move || transport::listen(&transport, &stop, &delivery)
        });
    match spawned {
        Ok(thread) => Outcome::Primary(Guard {
            delivery,
            stop,
            wake,
            thread: Some(thread),
            files,
            _lock: lock,
        }),
        Err(error) => {
            for file in &files {
                let _ = std::fs::remove_file(file);
            }
            Outcome::Unguarded(format!("cannot start the listener thread: {error}"))
        }
    }
}
