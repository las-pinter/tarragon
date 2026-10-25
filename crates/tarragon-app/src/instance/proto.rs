//! The one-line wire protocol and the port file format. All of it is pure
//! except [`new_token`] and [`read_line`].

use std::io::{self, BufRead, BufReader, Read};

/// The request text without the token.
const ACTIVATE: &str = "tarragon/1 activate";

/// The reply to a valid request, with its line end.
pub const REPLY: &str = "ok\n";

/// The longest line we read. Longer input is cut off and ignored.
const MAX_LINE_BYTES: u64 = 256;

/// What a received line asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Request {
    /// Show the window.
    Activate,
    /// An activate request with a missing or wrong token.
    BadToken,
    /// Anything else.
    Unknown,
}

/// The request line to send, with its line end.
#[must_use]
pub fn request_line(token: Option<&str>) -> String {
    match token {
        Some(token) => format!("{ACTIVATE} {token}\n"),
        None => format!("{ACTIVATE}\n"),
    }
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
    match (token, line.strip_prefix(ACTIVATE)) {
        (None, Some("")) => Request::Activate,
        (Some(expected), Some(rest)) => match rest.strip_prefix(' ') {
            Some(given) if same_token(given, expected) => Request::Activate,
            Some(_) => Request::BadToken,
            None if rest.is_empty() => Request::BadToken,
            None => Request::Unknown,
        },
        _ => Request::Unknown,
    }
}

/// Read one line, at most [`MAX_LINE_BYTES`] bytes.
///
/// # Errors
///
/// Returns the read error, including a timeout.
pub fn read_line<R: Read>(reader: &mut R) -> io::Result<String> {
    let mut line = String::new();
    BufReader::new(reader.take(MAX_LINE_BYTES)).read_line(&mut line)?;
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
        let line = request_line(None);
        assert_eq!(line, "tarragon/1 activate\n");
        assert_eq!(parse_request(&line, None), Request::Activate);
    }

    #[test]
    fn request_line_round_trips_with_a_token() {
        let line = request_line(Some("abc"));
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
    fn read_line_stops_at_the_line_end_and_the_size_cap() {
        let mut input: &[u8] = b"one\ntwo\n";
        assert_eq!(read_line(&mut input).expect("read"), "one\n");
        let long = vec![b'x'; 1000];
        let mut input: &[u8] = &long;
        assert_eq!(read_line(&mut input).expect("read").len(), 256);
    }
}
