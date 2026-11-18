//! The one-line wire protocol and the port file format. All of it is pure
//! except [`new_token`] and [`read_line`].

use std::io::{self, BufRead, BufReader, Read};
use std::path::{Path, PathBuf};

/// The activate request text without the token.
const ACTIVATE: &str = "tarragon/1 activate";

/// The open request text without the token and the folder.
const OPEN: &str = "tarragon/1 open";

/// The longest folder path an open request carries, in bytes.
pub const MAX_PATH_BYTES: usize = 4096;

/// The reply to a valid request, with its line end.
pub const REPLY: &str = "ok\n";

/// The longest line we read: the longest path and room for the rest. A line
/// that reaches it without a line end is refused.
const MAX_LINE_BYTES: u64 = 4352;

/// What a received line asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    /// Show the window.
    Activate,
    /// Open this absolute folder and show the window.
    Open(PathBuf),
    /// A request with a missing or wrong token.
    BadToken,
    /// Anything else.
    Unknown,
}

/// The request line to send, with its line end. With a folder it is an open
/// request; the folder must have passed [`sendable_path`].
#[must_use]
pub fn request_line(token: Option<&str>, folder: Option<&str>) -> String {
    let head = if folder.is_some() { OPEN } else { ACTIVATE };
    let mut line = head.to_owned();
    for part in [token, folder].into_iter().flatten() {
        line.push(' ');
        line.push_str(part);
    }
    line.push('\n');
    line
}

/// The folder as line text, or why it cannot travel in a request.
///
/// # Errors
///
/// Returns the reason when the path is not UTF-8, holds a line break, or is
/// longer than [`MAX_PATH_BYTES`].
pub fn sendable_path(folder: &Path) -> Result<&str, String> {
    let text = folder.to_str().ok_or("the path is not UTF-8")?;
    if text.contains(['\r', '\n']) {
        return Err("the path holds a line break".into());
    }
    if text.len() > MAX_PATH_BYTES {
        return Err(format!("the path is longer than {MAX_PATH_BYTES} bytes"));
    }
    Ok(text)
}

/// Compare two strings in time that does not depend on where they differ.
fn same_token(given: &str, expected: &str) -> bool {
    given.len() == expected.len()
        && given
            .bytes()
            .zip(expected.bytes())
            .fold(0_u8, |diff, (a, b)| diff | (a ^ b))
            == 0
}

/// Read a received line (without its line end). `token` is the token the
/// listener expects, or `None` when the transport needs none.
#[must_use]
pub fn parse_request(line: &str, token: Option<&str>) -> Request {
    let line = line.trim_end_matches(['\r', '\n']);
    if let Some(rest) = line.strip_prefix(ACTIVATE) {
        return match (token, rest.strip_prefix(' ')) {
            (None, _) if rest.is_empty() => Request::Activate,
            (Some(expected), Some(given)) if same_token(given, expected) => Request::Activate,
            (Some(_), Some(_)) => Request::BadToken,
            (Some(_), None) if rest.is_empty() => Request::BadToken,
            _ => Request::Unknown,
        };
    }
    let Some(rest) = line
        .strip_prefix(OPEN)
        .and_then(|rest| rest.strip_prefix(' '))
    else {
        return Request::Unknown;
    };
    let folder = match token {
        None => rest,
        Some(expected) => match rest.split_once(' ') {
            Some((given, folder)) if same_token(given, expected) => folder,
            _ => return Request::BadToken,
        },
    };
    let folder = Path::new(folder);
    if folder.is_absolute() {
        Request::Open(folder.to_path_buf())
    } else {
        Request::Unknown
    }
}

/// Read one line, at most [`MAX_LINE_BYTES`] bytes.
///
/// # Errors
///
/// Returns the read error, including a timeout, and an error for a line that
/// fills the cap without a line end or is not UTF-8.
pub fn read_line<R: Read>(reader: &mut R) -> io::Result<String> {
    let mut line = String::new();
    let read = BufReader::new(reader.take(MAX_LINE_BYTES)).read_line(&mut line)?;
    if read as u64 == MAX_LINE_BYTES && !line.ends_with('\n') {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "the request has no line end within the size cap",
        ));
    }
    Ok(line)
}

/// A random token of 32 hex characters. The hasher state comes from the OS.
/// The pid, the time and a counter are mixed in so two calls differ.
#[must_use]
#[cfg(any(not(unix), test))]
pub fn new_token() -> String {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_nanos());
    let count = COUNTER.fetch_add(1, Ordering::Relaxed);
    let part = |salt: u64| {
        let mut hasher = RandomState::new().build_hasher();
        hasher.write_u32(std::process::id());
        hasher.write_u128(nanos);
        hasher.write_u64(count);
        hasher.write_u64(salt);
        hasher.finish()
    };
    format!("{:016x}{:016x}", part(0), part(1))
}

/// The text of the port file.
#[must_use]
#[cfg(any(not(unix), test))]
pub fn format_port_file(port: u16, token: &str) -> String {
    format!("{port} {token}\n")
}

/// Read the port file. A port of 0 or a missing token is rejected.
#[must_use]
#[cfg(any(not(unix), test))]
pub fn parse_port_file(text: &str) -> Option<(u16, String)> {
    let mut parts = text.split_whitespace();
    let port = parts
        .next()?
        .parse::<u16>()
        .ok()
        .filter(|port| *port != 0)?;
    let token = parts.next()?;
    parts.next().is_none().then(|| (port, token.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_line_round_trips_without_a_token() {
        let line = request_line(None, None);
        assert_eq!(line, "tarragon/1 activate\n");
        assert_eq!(parse_request(&line, None), Request::Activate);
    }

    #[test]
    fn request_line_round_trips_with_a_token() {
        let line = request_line(Some("abc"), None);
        assert_eq!(line, "tarragon/1 activate abc\n");
        assert_eq!(parse_request(&line, Some("abc")), Request::Activate);
    }

    #[test]
    fn wrong_or_missing_token_is_rejected() {
        assert_eq!(
            parse_request("tarragon/1 activate bad", Some("abc")),
            Request::BadToken
        );
        assert_eq!(
            parse_request("tarragon/1 activate", Some("abc")),
            Request::BadToken
        );
        assert_eq!(
            parse_request("tarragon/1 activate abcd", Some("abc")),
            Request::BadToken
        );
    }

    #[test]
    fn open_line_round_trips_with_and_without_a_token() {
        // An absolute path on every system, with a space and a non-ASCII name.
        let base = std::env::temp_dir().join("a b").join("ref ü");
        let folder = base.to_str().expect("a UTF-8 temp path");
        let line = request_line(None, Some(folder));
        assert_eq!(line, format!("tarragon/1 open {folder}\n"));
        assert_eq!(parse_request(&line, None), Request::Open(folder.into()));
        let line = request_line(Some("abc"), Some(folder));
        assert_eq!(line, format!("tarragon/1 open abc {folder}\n"));
        assert_eq!(
            parse_request(&line, Some("abc")),
            Request::Open(folder.into())
        );
    }

    #[test]
    fn open_with_a_bad_token_or_folder_is_refused() {
        assert_eq!(
            parse_request("tarragon/1 open bad /x", Some("abc")),
            Request::BadToken
        );
        assert_eq!(
            parse_request("tarragon/1 open /x", Some("abc")),
            Request::BadToken
        );
        assert_eq!(parse_request("tarragon/1 open x", None), Request::Unknown);
        assert_eq!(parse_request("tarragon/1 open ", None), Request::Unknown);
        assert_eq!(parse_request("tarragon/1 open", None), Request::Unknown);
        assert_eq!(parse_request("tarragon/1 openx /x", None), Request::Unknown);
    }

    #[test]
    fn only_a_short_utf8_path_without_line_breaks_is_sendable() {
        assert_eq!(sendable_path(Path::new("/a b")), Ok("/a b"));
        assert!(sendable_path(Path::new("/a\nb")).is_err());
        assert!(sendable_path(Path::new("/a\rb")).is_err());
        let longest = format!("/{}", "x".repeat(MAX_PATH_BYTES - 1));
        assert!(sendable_path(Path::new(&longest)).is_ok());
        assert!(sendable_path(Path::new(&format!("{longest}x"))).is_err());
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStrExt;
            let bytes = std::ffi::OsStr::from_bytes(b"/a\xffb");
            assert!(sendable_path(Path::new(bytes)).is_err());
        }
    }

    #[test]
    fn the_longest_open_line_fits_the_cap() {
        let longest = format!("/{}", "x".repeat(MAX_PATH_BYTES - 1));
        let line = request_line(Some(&"f".repeat(32)), Some(&longest));
        let mut input: &[u8] = line.as_bytes();
        assert_eq!(read_line(&mut input).expect("read"), line);
    }

    #[test]
    fn unknown_lines_are_unknown() {
        assert_eq!(parse_request("hello", None), Request::Unknown);
        assert_eq!(parse_request("hello", Some("abc")), Request::Unknown);
        assert_eq!(
            parse_request("tarragon/1 activate abc", None),
            Request::Unknown
        );
        assert_eq!(parse_request("", None), Request::Unknown);
    }

    #[test]
    fn tokens_are_long_hex_and_differ() {
        let a = new_token();
        assert_eq!(a.len(), 32);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(a, new_token());
    }

    #[test]
    fn port_file_round_trips() {
        let text = format_port_file(4242, "tok");
        assert_eq!(parse_port_file(&text), Some((4242, "tok".to_owned())));
    }

    #[test]
    fn bad_port_files_are_rejected() {
        for text in ["", "4242", "0 tok", "70000 tok", "x tok", "1 tok extra"] {
            assert_eq!(parse_port_file(text), None, "{text:?}");
        }
    }

    #[test]
    fn read_line_stops_at_the_line_end_and_refuses_a_line_over_the_cap() {
        let mut input: &[u8] = b"one\ntwo\n";
        assert_eq!(read_line(&mut input).expect("read"), "one\n");
        let long = vec![b'x'; 5000];
        let mut input: &[u8] = &long;
        let error = read_line(&mut input).expect_err("too long");
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    }
}
