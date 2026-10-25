//! The unix socket transport, for Linux and macOS.

use std::io;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::time::Duration;

use super::paths::Layout;
use super::transport::{IO_TIMEOUT, Transport, WakeFn};

/// A listener on a unix socket file.
pub struct UnixTransport {
    /// The bound listener.
    listener: UnixListener,
    /// The socket path, for the wake-up connect.
    path: PathBuf,
}

/// Put the write timeout on a stream. Reads get a deadline per request.
fn with_timeouts(stream: UnixStream) -> io::Result<UnixStream> {
    stream.set_write_timeout(Some(IO_TIMEOUT))?;
    Ok(stream)
}

impl Transport for UnixTransport {
    type Stream = UnixStream;

    fn bind(layout: &Layout) -> io::Result<Self> {
        Ok(Self {
            listener: UnixListener::bind(&layout.sock)?,
            path: layout.sock.clone(),
        })
    }

    fn token(&self) -> Option<&str> {
        None
    }

    fn accept(&self) -> io::Result<UnixStream> {
        with_timeouts(self.listener.accept()?.0)
    }

    fn set_read_timeout(stream: &UnixStream, timeout: Duration) -> io::Result<()> {
        stream.set_read_timeout(Some(timeout))
    }

    fn waker(&self) -> WakeFn {
        let path = self.path.clone();
        // std has no connect timeout for unix sockets. The connect only waits
        // when the backlog is full, and the listener drains it because each
        // request has a total deadline.
        Box::new(move || UnixStream::connect(&path).is_ok())
    }

    fn connect(layout: &Layout) -> io::Result<(UnixStream, Option<String>)> {
        Ok((with_timeouts(UnixStream::connect(&layout.sock)?)?, None))
    }

    fn files(layout: &Layout) -> Vec<PathBuf> {
        vec![layout.sock.clone()]
    }
}
