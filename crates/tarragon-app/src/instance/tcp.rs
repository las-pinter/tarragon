//! The loopback TCP transport, for systems without unix sockets. The port
//! and a random token sit in a file beside the lock. Only a client that can
//! read that file knows the token.

use std::io;
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::time::Duration;

use super::paths::Layout;
use super::proto;
use super::transport::{IO_TIMEOUT, Transport, WakeFn};

/// A listener on a loopback port.
pub struct TcpTransport {
    /// The bound listener.
    listener: TcpListener,
    /// The address, for the wake-up connect.
    addr: SocketAddr,
    /// The token a request must carry.
    token: String,
}

/// Put the write timeout on a stream. Reads get a deadline per request.
fn with_timeouts(stream: TcpStream) -> io::Result<TcpStream> {
    stream.set_write_timeout(Some(IO_TIMEOUT))?;
    Ok(stream)
}

/// The temp name used to write the port file before the rename.
fn temp_name(port_file: &Path) -> PathBuf {
    port_file.with_extension("tmp")
}

/// Write the port file in one step: a temp file, then a rename, so a client
/// never reads half of it.
fn write_port_file(path: &Path, port: u16, token: &str) -> io::Result<()> {
    let temp = temp_name(path);
    std::fs::write(&temp, proto::format_port_file(port, token))?;
    std::fs::rename(&temp, path)
}

impl Transport for TcpTransport {
    type Stream = TcpStream;

    fn bind(layout: &Layout) -> io::Result<Self> {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
        let addr = listener.local_addr()?;
        let token = proto::new_token();
        write_port_file(&layout.port, addr.port(), &token)?;
        Ok(Self {
            listener,
            addr,
            token,
        })
    }

    fn token(&self) -> Option<&str> {
        Some(&self.token)
    }

    fn accept(&self) -> io::Result<TcpStream> {
        with_timeouts(self.listener.accept()?.0)
    }

    fn set_read_timeout(stream: &TcpStream, timeout: Duration) -> io::Result<()> {
        stream.set_read_timeout(Some(timeout))
    }

    fn waker(&self) -> WakeFn {
        let addr = self.addr;
        Box::new(move || TcpStream::connect_timeout(&addr, IO_TIMEOUT).is_ok())
    }

    fn connect(layout: &Layout) -> io::Result<(TcpStream, Option<String>)> {
        let text = std::fs::read_to_string(&layout.port)?;
        let (port, token) = proto::parse_port_file(&text)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "bad port file"))?;
        let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
        let stream = TcpStream::connect_timeout(&addr, IO_TIMEOUT)?;
        Ok((with_timeouts(stream)?, Some(token)))
    }

    fn files(layout: &Layout) -> Vec<PathBuf> {
        vec![layout.port.clone(), temp_name(&layout.port)]
    }
}
