//! The transport seam, the listener loop, and the client side.

use std::fs::File;
use std::io::{self, Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use super::Delivery;
use super::paths::Layout;
use super::proto::{self, Request};

/// The write timeout, and the time one whole request or reply may take.
pub const IO_TIMEOUT: Duration = Duration::from_millis(500);

/// How long a second start retries while the primary may still be binding.
const FORWARD_WINDOW: Duration = Duration::from_secs(2);

/// The pause between two connect attempts.
const RETRY_PAUSE: Duration = Duration::from_millis(50);

/// A function that opens one connection to wake a blocked accept. It returns
/// whether the connect worked.
pub type WakeFn = Box<dyn Fn() -> bool + Send + Sync>;

/// One way to carry the activate line between two processes.
pub trait Transport: Sized + Send + 'static {
    /// The connection type. The write timeout is already set on it.
    type Stream: Read + Write + Send;

    /// Start listening.
    ///
    /// # Errors
    ///
    /// Returns the bind or file error.
    fn bind(layout: &Layout) -> io::Result<Self>;

    /// The token a request must carry, if the transport uses one.
    fn token(&self) -> Option<&str>;

    /// Wait for one connection.
    ///
    /// # Errors
    ///
    /// Returns the accept error.
    fn accept(&self) -> io::Result<Self::Stream>;

    /// Set the read timeout of a stream.
    ///
    /// # Errors
    ///
    /// Returns the socket error.
    fn set_read_timeout(stream: &Self::Stream, timeout: Duration) -> io::Result<()>;

    /// A function that connects to this listener, to end a blocked accept.
    fn waker(&self) -> WakeFn;

    /// Connect to the listener of another process. The token to send comes
    /// back with the stream.
    ///
    /// # Errors
    ///
    /// Returns the error when no listener answers.
    fn connect(layout: &Layout) -> io::Result<(Self::Stream, Option<String>)>;

    /// The files this transport leaves behind.
    fn files(layout: &Layout) -> Vec<std::path::PathBuf>;
}

/// The longest pause after a failed accept.
const MAX_BACKOFF: Duration = Duration::from_secs(1);

/// Accept connections until `stop` is set.
pub fn listen<T: Transport>(transport: &T, stop: &AtomicBool, delivery: &Delivery) {
    let mut backoff = RETRY_PAUSE;
    let mut failing = false;
    loop {
        let accepted = transport.accept();
        if stop.load(Ordering::Acquire) {
            return;
        }
        match accepted {
            Ok(stream) => {
                failing = false;
                backoff = RETRY_PAUSE;
                serve::<T>(stream, transport.token(), delivery);
            }
            Err(error) => {
                // One warning per streak, and a growing pause, so a lasting
                // error does not flood the log or burn the CPU.
                if !failing {
                    tracing::warn!("instance listener: accept failed: {error}");
                }
                failing = true;
                std::thread::sleep(backoff);
                backoff = (backoff * 2).min(MAX_BACKOFF);
            }
        }
    }
}

/// A reader that gives the whole read one deadline. The socket timeout alone
/// applies to each call, so a client that sends a byte now and then could
/// hold the listener for a long time.
struct Deadline<'a, T: Transport> {
    /// The connection.
    stream: &'a mut T::Stream,
    /// When the read must be done.
    end: Instant,
}

impl<T: Transport> Read for Deadline<'_, T> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let left = self.end.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return Err(io::ErrorKind::TimedOut.into());
        }
        T::set_read_timeout(self.stream, left)?;
        self.stream.read(buf)
    }
}

/// Read one line from `stream` within [`IO_TIMEOUT`] in total.
fn read_request<T: Transport>(stream: &mut T::Stream) -> io::Result<String> {
    let mut reader = Deadline::<T> {
        stream,
        end: Instant::now() + IO_TIMEOUT,
    };
    proto::read_line(&mut reader)
}

/// Answer one connection.
fn serve<T: Transport>(mut stream: T::Stream, token: Option<&str>, delivery: &Delivery) {
    let line = match read_request::<T>(&mut stream) {
        Ok(line) => line,
        Err(error) => {
            tracing::debug!("instance listener: cannot read a request: {error}");
            return;
        }
    };
    match proto::parse_request(&line, token) {
        Request::Activate => {
            delivery.signal();
            if let Err(error) = stream.write_all(proto::REPLY.as_bytes()) {
                tracing::debug!("instance listener: cannot send the reply: {error}");
            }
        }
        Request::BadToken => {
            tracing::warn!("instance listener: ignored a request with a bad token");
        }
        Request::Unknown => {
            tracing::warn!("instance listener: ignored an unknown request");
        }
    }
}

/// How a forward ended.
pub enum Forwarded {
    /// The primary answered.
    Sent,
    /// The primary went away and this process now holds the lock.
    TookLock,
    /// No attempt worked in the time window.
    Failed(io::Error),
}

/// Send the activate request to the primary, retrying for a short time
/// because it may still be binding. After each failed try the lock is tried
/// too: if the primary quit meanwhile, this start must become the primary
/// rather than leave nothing running.
pub fn forward<T: Transport>(layout: &Layout, lock: &File) -> Forwarded {
    let deadline = Instant::now() + FORWARD_WINDOW;
    loop {
        let error = match try_once::<T>(layout) {
            Ok(()) => return Forwarded::Sent,
            Err(error) => error,
        };
        if lock.try_lock().is_ok() {
            return Forwarded::TookLock;
        }
        if Instant::now() >= deadline {
            return Forwarded::Failed(error);
        }
        std::thread::sleep(RETRY_PAUSE);
    }
}

/// One attempt: connect, send the line, wait for `ok`.
fn try_once<T: Transport>(layout: &Layout) -> io::Result<()> {
    let (mut stream, token) = T::connect(layout)?;
    stream.write_all(proto::request_line(token.as_deref()).as_bytes())?;
    stream.flush()?;
    if read_request::<T>(&mut stream)? == proto::REPLY {
        Ok(())
    } else {
        Err(io::Error::new(io::ErrorKind::InvalidData, "no ok reply"))
    }
}
