//! Copy files to the system clipboard and read files from it.
//!
//! The clipboard holds one target, `text/uri-list`. A helper program does the
//! work: `wl-copy` and `wl-paste` under Wayland, `xclip` otherwise.

use std::borrow::Cow;
use std::ffi::{OsStr, OsString};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

/// The clipboard target for a list of files.
const URI_LIST: &str = "text/uri-list";
/// The start of a local file address.
const FILE_SCHEME: &[u8] = b"file:";
/// How long a copy or a paste may take.
const TOOL_TIMEOUT: Duration = Duration::from_secs(2);
/// How long to wait for the error text of a helper that has ended.
const STDERR_WAIT: Duration = Duration::from_millis(100);
/// How often to check whether a helper has ended.
const POLL_STEP: Duration = Duration::from_millis(10);
/// The most bytes of a file list that a paste reads.
const MAX_LIST_BYTES: usize = 1 << 20;
/// The most bytes of helper error text that a paste keeps.
const MAX_STDERR_BYTES: usize = 1024;

/// Why a clipboard call failed.
#[derive(Debug, thiserror::Error)]
pub enum ClipboardError {
    /// The helper program is not installed. The text names its package.
    #[error("the clipboard needs {0}; install the {0} package")]
    ToolMissing(&'static str),
    /// This system has no clipboard support yet.
    #[error("the clipboard is not supported yet on this system")]
    Unsupported,
    /// Neither a Wayland nor an X11 display is set.
    #[error("no display for the clipboard: neither WAYLAND_DISPLAY nor DISPLAY is set")]
    NoDisplay,
    /// The helper did not finish in time.
    #[error("the clipboard did not answer in time")]
    Timeout,
    /// A helper program could not be started, written to, or read from.
    #[error("cannot use `{program}`: {source}")]
    Io {
        /// The helper program.
        program: &'static str,
        /// The error from the operating system.
        source: io::Error,
    },
    /// A helper program ended with an error.
    #[error("`{program}` failed: {status}")]
    Failed {
        /// The helper program.
        program: &'static str,
        /// How the program ended.
        status: ExitStatus,
    },
}

/// A helper program, its arguments, and the package that provides it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ToolCommand {
    /// The program to run.
    program: &'static str,
    /// The arguments.
    args: &'static [&'static str],
    /// The package to name when the program is missing.
    package: &'static str,
}

/// Put `paths` on the clipboard as a list of files.
///
/// A path that is not absolute is left out. The helper keeps the clipboard
/// after this call returns.
///
/// # Errors
///
/// Returns [`ClipboardError::ToolMissing`] when the helper is not installed,
/// [`ClipboardError::Unsupported`] on Windows, [`ClipboardError::NoDisplay`]
/// when no display is set, [`ClipboardError::Timeout`] when the helper stalls,
/// and an I/O or failure error when the helper does not take the list.
pub fn copy_files(paths: &[PathBuf]) -> Result<(), ClipboardError> {
    if cfg!(windows) {
        return Err(ClipboardError::Unsupported);
    }
    let (wayland, x11) = display_env();
    let tool = copy_command(wayland.as_deref(), x11.as_deref())?;
    copy_with(&tool, paths, TOOL_TIMEOUT)
}

/// Read the files on the clipboard.
///
/// Only `file://` entries come back, and no file type is checked. A clipboard
/// with no file list gives an empty list.
///
/// # Errors
///
/// Returns [`ClipboardError::ToolMissing`] when the helper is not installed,
/// [`ClipboardError::Unsupported`] on Windows, [`ClipboardError::NoDisplay`]
/// when no display is set, and [`ClipboardError::Timeout`] when the clipboard
/// owner does not answer.
pub fn read_files() -> Result<Vec<PathBuf>, ClipboardError> {
    if cfg!(windows) {
        return Err(ClipboardError::Unsupported);
    }
    let (wayland, x11) = display_env();
    let tool = paste_command(wayland.as_deref(), x11.as_deref())?;
    read_with(&tool, TOOL_TIMEOUT)
}

/// The values of `WAYLAND_DISPLAY` and `DISPLAY`, each when set and not empty.
fn display_env() -> (Option<OsString>, Option<OsString>) {
    let get = |name| std::env::var_os(name).filter(|value| !value.is_empty());
    (get("WAYLAND_DISPLAY"), get("DISPLAY"))
}

/// The program that puts a file list on the clipboard. The arguments are the
/// values of `WAYLAND_DISPLAY` and `DISPLAY`.
fn copy_command(
    wayland_display: Option<&OsStr>,
    x11_display: Option<&OsStr>,
) -> Result<ToolCommand, ClipboardError> {
    if wayland_display.is_some() {
        Ok(ToolCommand {
            program: "wl-copy",
            args: &["--type", URI_LIST],
            package: "wl-clipboard",
        })
    } else if x11_display.is_some() {
        Ok(ToolCommand {
            program: "xclip",
            args: &["-selection", "clipboard", "-t", URI_LIST, "-i"],
            package: "xclip",
        })
    } else {
        Err(ClipboardError::NoDisplay)
    }
}

/// The program that prints the file list on the clipboard. The arguments are
/// the values of `WAYLAND_DISPLAY` and `DISPLAY`.
fn paste_command(
    wayland_display: Option<&OsStr>,
    x11_display: Option<&OsStr>,
) -> Result<ToolCommand, ClipboardError> {
    if wayland_display.is_some() {
        Ok(ToolCommand {
            program: "wl-paste",
            args: &["--type", URI_LIST],
            package: "wl-clipboard",
        })
    } else if x11_display.is_some() {
        Ok(ToolCommand {
            program: "xclip",
            args: &["-selection", "clipboard", "-o", "-t", URI_LIST],
            package: "xclip",
        })
    } else {
        Err(ClipboardError::NoDisplay)
    }
}

/// Map a failed process start to an error.
fn start_error(tool: &ToolCommand, source: io::Error) -> ClipboardError {
    if source.kind() == io::ErrorKind::NotFound {
        ClipboardError::ToolMissing(tool.package)
    } else {
        io_error(tool, source)
    }
}

/// Wrap an I/O error with the program name.
fn io_error(tool: &ToolCommand, source: io::Error) -> ClipboardError {
    ClipboardError::Io {
        program: tool.program,
        source,
    }
}

/// Map a failed wait on a reader thread to an error.
fn recv_error(tool: &ToolCommand, error: RecvTimeoutError) -> ClipboardError {
    match error {
        RecvTimeoutError::Timeout => ClipboardError::Timeout,
        RecvTimeoutError::Disconnected => io_error(tool, io::Error::other("the reader stopped")),
    }
}

/// The time left before `deadline`.
fn remaining(deadline: Instant) -> Duration {
    deadline.saturating_duration_since(Instant::now())
}

/// End a child and collect it, so it does not stay a zombie.
fn kill_and_reap(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

/// Wait for `child` to end until `deadline`. `None` means the time ran out.
fn wait_until(child: &mut Child, deadline: Instant) -> io::Result<Option<ExitStatus>> {
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(Some(status));
        }
        if Instant::now() >= deadline {
            return Ok(None);
        }
        std::thread::sleep(POLL_STEP);
    }
}

/// Give `paths` to `tool` on its standard input, within `timeout`.
///
/// The helper forks a child that keeps the clipboard. Its output goes nowhere,
/// so this call waits for the parent process only.
fn copy_with(
    tool: &ToolCommand,
    paths: &[PathBuf],
    timeout: Duration,
) -> Result<(), ClipboardError> {
    let text = uri_list(paths);
    if text.is_empty() {
        return Ok(());
    }
    let deadline = Instant::now() + timeout;
    let mut child = Command::new(tool.program)
        .args(tool.args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|source| start_error(tool, source))?;
    let written = match start_writer(&mut child, text) {
        Ok(written) => written,
        Err(source) => {
            kill_and_reap(&mut child);
            return Err(io_error(tool, source));
        }
    };
    let result = finish_copy(tool, &mut child, &written, deadline);
    if result.is_err() {
        // A stalled helper ends here, and its closed pipe ends the writer.
        kill_and_reap(&mut child);
    }
    result
}

/// Write `text` to the standard input of `child` from a thread, so a helper
/// that stops reading cannot block the caller.
fn start_writer(child: &mut Child, text: String) -> io::Result<Receiver<io::Result<()>>> {
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| io::Error::other("no input pipe"))?;
    let (sender, receiver) = mpsc::channel();
    std::thread::Builder::new()
        .name("tarragon-clipboard-write".into())
        .spawn(move || {
            let result = stdin.write_all(text.as_bytes());
            // The helper sees the end of its input when the pipe closes.
            drop(stdin);
            let _ = sender.send(result);
        })?;
    Ok(receiver)
}

/// Wait for the helper to end, then report how the copy went. A bad exit
/// status wins over a write error, which a dead helper causes.
fn finish_copy(
    tool: &ToolCommand,
    child: &mut Child,
    written: &Receiver<io::Result<()>>,
    deadline: Instant,
) -> Result<(), ClipboardError> {
    let status = wait_until(child, deadline)
        .map_err(|source| io_error(tool, source))?
        .ok_or(ClipboardError::Timeout)?;
    if !status.success() {
        return Err(ClipboardError::Failed {
            program: tool.program,
            status,
        });
    }
    // A helper that ended well has read all its input, so the writer is done.
    let result = written.recv_timeout(remaining(deadline)).unwrap_or(Ok(()));
    result.map_err(|source| io_error(tool, source))
}

/// Run `tool` and parse its output as a file list, within `timeout`.
///
/// A helper that exits with an error has no file list to give, so the result
/// is empty and the error text goes to the log.
fn read_with(tool: &ToolCommand, timeout: Duration) -> Result<Vec<PathBuf>, ClipboardError> {
    let deadline = Instant::now() + timeout;
    let mut child = Command::new(tool.program)
        .args(tool.args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|source| start_error(tool, source))?;
    let result = match start_readers(&mut child) {
        Ok((out, err)) => finish_read(tool, &mut child, &out, &err, deadline),
        Err(source) => Err(io_error(tool, source)),
    };
    if result.is_err() {
        // A stalled helper ends here, and its closed pipes end the readers.
        kill_and_reap(&mut child);
    }
    result
}

/// Start a reader thread for the output pipe and one for the error pipe.
#[allow(clippy::type_complexity, reason = "a pair of reader channels")]
fn start_readers(
    child: &mut Child,
) -> io::Result<(Receiver<io::Result<Vec<u8>>>, Receiver<io::Result<Vec<u8>>>)> {
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| io::Error::other("no output pipe"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| io::Error::other("no error pipe"))?;
    Ok((
        read_capped("tarragon-clipboard-read", stdout, MAX_LIST_BYTES)?,
        read_capped("tarragon-clipboard-err", stderr, MAX_STDERR_BYTES)?,
    ))
}

/// Read up to `cap` bytes of `reader` on a thread. The rest is read and thrown
/// away, so the helper does not stop on a full pipe.
fn read_capped<R: Read + Send + 'static>(
    name: &str,
    mut reader: R,
    cap: usize,
) -> io::Result<Receiver<io::Result<Vec<u8>>>> {
    let (sender, receiver) = mpsc::channel();
    let limit = u64::try_from(cap).unwrap_or(u64::MAX);
    std::thread::Builder::new()
        .name(name.into())
        .spawn(move || {
            let mut bytes = Vec::new();
            let result = (&mut reader)
                .take(limit)
                .read_to_end(&mut bytes)
                .and_then(|_| io::copy(&mut reader, &mut io::sink()))
                .map(|_| bytes);
            let _ = sender.send(result);
        })?;
    Ok(receiver)
}

/// Wait for the output and for the end of the helper, both within `deadline`,
/// then parse the output.
fn finish_read(
    tool: &ToolCommand,
    child: &mut Child,
    out: &Receiver<io::Result<Vec<u8>>>,
    err: &Receiver<io::Result<Vec<u8>>>,
    deadline: Instant,
) -> Result<Vec<PathBuf>, ClipboardError> {
    let mut bytes = out
        .recv_timeout(remaining(deadline))
        .map_err(|error| recv_error(tool, error))?
        .map_err(|source| io_error(tool, source))?;
    let status = wait_until(child, deadline)
        .map_err(|source| io_error(tool, source))?
        .ok_or(ClipboardError::Timeout)?;
    if !status.success() {
        let text = err
            .recv_timeout(STDERR_WAIT)
            .ok()
            .and_then(Result::ok)
            .unwrap_or_default();
        tracing::warn!(
            "`{}` ended with {status}; no file list. {}",
            tool.program,
            String::from_utf8_lossy(&text).trim()
        );
        return Ok(Vec::new());
    }
    if bytes.len() >= MAX_LIST_BYTES {
        // The last line may be cut, so it is left out.
        let end = bytes
            .iter()
            .rposition(|&byte| byte == b'\n')
            .map_or(0, |i| i + 1);
        bytes.truncate(end);
    }
    Ok(parse_uri_list(&bytes))
}

/// The `text/uri-list` text for `paths`: one percent-encoded `file://` line
/// each, ending in CRLF. A path that is not absolute is left out.
fn uri_list(paths: &[PathBuf]) -> String {
    let mut text = String::new();
    for path in paths.iter().filter(|path| path.is_absolute()) {
        text.push_str("file://");
        encode_into(&mut text, &path_bytes(path));
        text.push_str("\r\n");
    }
    text
}

/// The bytes of a path. On Unix they are the exact bytes of the name.
#[cfg(unix)]
fn path_bytes(path: &Path) -> Cow<'_, [u8]> {
    use std::os::unix::ffi::OsStrExt;
    Cow::Borrowed(path.as_os_str().as_bytes())
}

/// The bytes of a path. A name that is not valid text loses its bad parts.
#[cfg(not(unix))]
fn path_bytes(path: &Path) -> Cow<'_, [u8]> {
    Cow::Owned(path.to_string_lossy().into_owned().into_bytes())
}

/// A path from decoded bytes.
#[cfg(unix)]
fn bytes_to_path(bytes: Vec<u8>) -> PathBuf {
    use std::os::unix::ffi::OsStringExt;
    PathBuf::from(OsString::from_vec(bytes))
}

/// A path from decoded bytes. A name that is not valid text loses its bad parts.
#[cfg(not(unix))]
fn bytes_to_path(bytes: Vec<u8>) -> PathBuf {
    PathBuf::from(String::from_utf8_lossy(&bytes).into_owned())
}

/// Add `bytes` to `out`, percent-encoded. Letters, digits, `-`, `.`, `_`, `~`,
/// and `/` stay as they are.
fn encode_into(out: &mut String, bytes: &[u8]) {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    for &byte in bytes {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~' | b'/') {
            out.push(char::from(byte));
        } else {
            out.push('%');
            out.push(char::from(HEX[usize::from(byte >> 4)]));
            out.push(char::from(HEX[usize::from(byte & 0x0F)]));
        }
    }
}

/// Undo percent-encoding. A `%` without two hex digits gives `None`.
fn decode(bytes: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let high = hex_value(*bytes.get(index + 1)?)?;
            let low = hex_value(*bytes.get(index + 2)?)?;
            out.push(high << 4 | low);
            index += 3;
        } else {
            out.push(bytes[index]);
            index += 1;
        }
    }
    Some(out)
}

/// The value of one hex digit.
fn hex_value(digit: u8) -> Option<u8> {
    char::from(digit)
        .to_digit(16)
        .and_then(|value| u8::try_from(value).ok())
}

/// The files in a `text/uri-list`. Comments, blank lines, other schemes, remote
/// hosts, and bad escapes are skipped.
fn parse_uri_list(bytes: &[u8]) -> Vec<PathBuf> {
    bytes
        .split(|&byte| byte == b'\n')
        .filter_map(|line| parse_line(line.strip_suffix(b"\r").unwrap_or(line)))
        .collect()
}

/// The path of one `file:` line, or `None`. Both `file:///path` and `file:/path`
/// are read.
fn parse_line(line: &[u8]) -> Option<PathBuf> {
    let rest = line.strip_prefix(FILE_SCHEME)?;
    let encoded = if let Some(authority) = rest.strip_prefix(b"//") {
        let path_start = authority.iter().position(|&byte| byte == b'/')?;
        let host = &authority[..path_start];
        if !host.is_empty() && !host.eq_ignore_ascii_case(b"localhost") {
            return None;
        }
        &authority[path_start..]
    } else if rest.starts_with(b"/") {
        rest
    } else {
        return None;
    };
    let bytes = decode(encoded)?;
    if bytes.contains(&0) {
        return None;
    }
    Some(bytes_to_path(bytes))
}

#[cfg(test)]
mod tests;
