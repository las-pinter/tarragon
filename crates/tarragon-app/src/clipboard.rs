//! Copy files to the system clipboard and read files from it.
//!
//! The work goes through `arboard` on Linux, Windows, and macOS. One thread
//! owns the clipboard for the whole run and serves the requests one at a time:
//! on Linux the copied list lives in this process and is served only while the
//! handle exists, and on Windows the clipboard may be opened by one thread at a
//! time. A caller waits at most [`ANSWER_TIMEOUT`] for the answer, so a stuck
//! clipboard cannot hold a copy or a paste for long.
//!
//! [`shutdown`] drops the handle at exit. On X11 the drop of the last handle
//! in the process offers the list to a running clipboard manager, which may
//! keep it after Tarragon closes.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};
use std::time::Duration;

use crossbeam_channel::{Receiver, RecvTimeoutError, Sender};

/// How long a caller waits for the clipboard thread to answer.
const ANSWER_TIMEOUT: Duration = Duration::from_secs(2);

/// Why a clipboard call failed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ClipboardError {
    /// The system offers no clipboard to this process, for example with no
    /// display on Linux. The text is the system's reason.
    #[error("the clipboard is not available: {0}")]
    Unavailable(String),
    /// Another program holds the clipboard right now.
    #[error("another program is using the clipboard")]
    Busy,
    /// The clipboard did not answer in time.
    #[error("the clipboard did not answer in time")]
    Timeout,
    /// None of the files can go on the clipboard: each one is missing, or its
    /// name is not valid text where the system needs text.
    #[error("none of these files can be put on the clipboard")]
    NothingToCopy,
    /// Any other clipboard error. The text is the system's reason.
    #[error("{0}")]
    Failed(String),
}

/// Put `paths` on the clipboard as a list of files.
///
/// A file that no longer exists is left out, and a link is put on the
/// clipboard as the file it points to.
///
/// # Errors
///
/// Returns [`ClipboardError::Unavailable`] when the system has no clipboard,
/// [`ClipboardError::Busy`] when another program holds it,
/// [`ClipboardError::NothingToCopy`] when no file is left to copy,
/// [`ClipboardError::Timeout`] when the clipboard does not answer in time, and
/// [`ClipboardError::Failed`] for any other error.
pub fn copy_files(paths: &[PathBuf]) -> Result<(), ClipboardError> {
    let paths = paths.to_vec();
    ask(|answer| Request::Copy(paths, answer), ANSWER_TIMEOUT)
}

/// Read the files on the clipboard. A clipboard that holds no file list gives
/// an empty list.
///
/// # Errors
///
/// Returns the errors of [`copy_files`], except
/// [`ClipboardError::NothingToCopy`].
pub fn read_files() -> Result<Vec<PathBuf>, ClipboardError> {
    ask(Request::Read, ANSWER_TIMEOUT)
}

/// Drop the clipboard handle and end the clipboard thread. The app calls it
/// at exit. It waits at most [`ANSWER_TIMEOUT`]. A later copy or paste starts
/// a new thread.
pub fn shutdown() {
    let taken = SERVICE
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .take();
    if let Some(service) = taken {
        service.stop(ANSWER_TIMEOUT);
    }
}

/// The clipboard thread of the app, started by the first request.
static SERVICE: Mutex<Option<Service>> = Mutex::new(None);

/// Send a request to the app's clipboard thread and wait for its answer.
fn ask<T: Send + 'static>(
    request: impl FnOnce(Sender<Result<T, ClipboardError>>) -> Request,
    timeout: Duration,
) -> Result<T, ClipboardError> {
    let requests = {
        let mut service = SERVICE.lock().unwrap_or_else(PoisonError::into_inner);
        if service.is_none() {
            *service = Some(Service::start(Box::new(|| {
                Arboard::new().map(|backend| Box::new(backend) as Box<dyn Backend>)
            }))?);
        }
        service.as_ref().map(Service::requests)
    };
    let Some(requests) = requests else {
        return Err(ClipboardError::Failed(
            "the clipboard thread is gone".into(),
        ));
    };
    send_and_wait(&requests, request, timeout)
}

/// Send one request and wait up to `timeout` for its answer.
fn send_and_wait<T>(
    requests: &Sender<Request>,
    request: impl FnOnce(Sender<Result<T, ClipboardError>>) -> Request,
    timeout: Duration,
) -> Result<T, ClipboardError> {
    let (answer, receive) = crossbeam_channel::bounded(1);
    requests
        .send(request(answer))
        .map_err(|_| ClipboardError::Failed("the clipboard thread is gone".into()))?;
    match receive.recv_timeout(timeout) {
        Ok(result) => result,
        Err(RecvTimeoutError::Timeout) => Err(ClipboardError::Timeout),
        Err(RecvTimeoutError::Disconnected) => Err(ClipboardError::Failed(
            "the clipboard thread ended without an answer".into(),
        )),
    }
}

/// One request to the clipboard thread, with the channel for its answer.
enum Request {
    Copy(Vec<PathBuf>, Sender<Result<(), ClipboardError>>),
    Read(Sender<Result<Vec<PathBuf>, ClipboardError>>),
}

/// What the clipboard thread does with the system clipboard. Tests use a fake.
trait Backend {
    /// Put the files on the clipboard.
    fn set_files(&mut self, paths: &[PathBuf]) -> Result<(), ClipboardError>;
    /// Read the files on the clipboard.
    fn files(&mut self) -> Result<Vec<PathBuf>, ClipboardError>;
}

/// Makes the backend on the clipboard thread. It runs again on the next
/// request after a failure, so a display that appears later is found.
type MakeBackend = Box<dyn FnMut() -> Result<Box<dyn Backend>, ClipboardError> + Send>;

/// A running clipboard thread.
struct Service {
    /// Where requests go. Dropping it ends the thread.
    requests: Sender<Request>,
    /// Closed by the thread when it ends.
    ended: Receiver<()>,
}

impl Service {
    /// Start the clipboard thread.
    fn start(make: MakeBackend) -> Result<Self, ClipboardError> {
        let (requests, incoming) = crossbeam_channel::unbounded::<Request>();
        let (done, ended) = crossbeam_channel::bounded::<()>(0);
        std::thread::Builder::new()
            .name("tarragon-clipboard".to_owned())
            .spawn(move || {
                serve(&incoming, make);
                drop(done);
            })
            .map_err(|error| {
                ClipboardError::Failed(format!("cannot start the clipboard: {error}"))
            })?;
        Ok(Self { requests, ended })
    }

    /// A sender for requests.
    fn requests(&self) -> Sender<Request> {
        self.requests.clone()
    }

    /// End the thread and wait for it up to `timeout`.
    fn stop(self, timeout: Duration) {
        let Self { requests, ended } = self;
        drop(requests);
        if ended.recv_timeout(timeout) == Err(RecvTimeoutError::Timeout) {
            tracing::warn!("the clipboard thread did not end in time");
        }
    }
}

/// Serve requests until every sender is gone, then drop the backend.
fn serve(incoming: &Receiver<Request>, mut make: MakeBackend) {
    let mut backend: Option<Box<dyn Backend>> = None;
    for request in incoming {
        if backend.is_none() {
            match make() {
                Ok(opened) => backend = Some(opened),
                Err(error) => {
                    tracing::warn!("cannot open the clipboard: {error}");
                    answer_with(request, &error);
                    continue;
                }
            }
        }
        let Some(clipboard) = backend.as_mut() else {
            continue;
        };
        // A caller that timed out dropped its receiver; its answer is not needed.
        match request {
            Request::Copy(paths, answer) => {
                let _ = answer.send(clipboard.set_files(&paths));
            }
            Request::Read(answer) => {
                let _ = answer.send(clipboard.files());
            }
        }
    }
}

/// Answer a request with `error`.
fn answer_with(request: Request, error: &ClipboardError) {
    match request {
        Request::Copy(_, answer) => {
            let _ = answer.send(Err(error.clone()));
        }
        Request::Read(answer) => {
            let _ = answer.send(Err(error.clone()));
        }
    }
}

/// The system clipboard through `arboard`.
struct Arboard(arboard::Clipboard);

impl Arboard {
    /// Open the system clipboard.
    fn new() -> Result<Self, ClipboardError> {
        arboard::Clipboard::new()
            .map(Self)
            .map_err(|error| ClipboardError::Unavailable(error.to_string()))
    }
}

impl Backend for Arboard {
    fn set_files(&mut self, paths: &[PathBuf]) -> Result<(), ClipboardError> {
        self.0.set().file_list(paths).map_err(|error| match error {
            arboard::Error::ConversionFailure => ClipboardError::NothingToCopy,
            other => map_error(other),
        })
    }

    fn files(&mut self) -> Result<Vec<PathBuf>, ClipboardError> {
        match self.0.get().file_list() {
            Ok(files) => Ok(local_paths(files)),
            // No file list on the clipboard is not an error for a paste.
            Err(arboard::Error::ContentNotAvailable | arboard::Error::ConversionFailure) => {
                Ok(Vec::new())
            }
            Err(other) => Err(map_error(other)),
        }
    }
}

/// The pasted entries as absolute paths.
///
/// On Linux arboard keeps the host of a `file://localhost/` address as the
/// first part of a relative path. That host is this machine, so the part goes.
/// Any other relative entry, such as one from a remote host, is left out: a
/// relative path would be looked up in the working folder.
fn local_paths(files: Vec<PathBuf>) -> Vec<PathBuf> {
    files
        .into_iter()
        .filter_map(|path| {
            if path.is_absolute() {
                return Some(path);
            }
            let rest = path.strip_prefix("localhost").ok()?;
            Some(Path::new("/").join(rest))
        })
        .filter(|path| path.is_absolute())
        .collect()
}

/// The app error for an `arboard` error.
fn map_error(error: arboard::Error) -> ClipboardError {
    match error {
        arboard::Error::ClipboardOccupied => ClipboardError::Busy,
        arboard::Error::ClipboardNotSupported => ClipboardError::Unavailable(error.to_string()),
        other => ClipboardError::Failed(other.to_string()),
    }
}

#[cfg(test)]
mod tests;
