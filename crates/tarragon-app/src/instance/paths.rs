//! Pure path helpers for the lock, the socket, and the port file.
//!
//! The caller passes the environment values in, so nothing here reads the
//! process environment or the file system.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

/// The longest socket path in bytes. The `sun_path` field is about 104 bytes
/// on macOS and 108 on Linux, so this stays under both.
pub const MAX_SOCKET_PATH_BYTES: usize = 100;

/// The FNV-1a 64-bit hash. It is written out here because the standard
/// hashers may change between Rust releases and two builds must agree.
#[must_use]
pub fn fnv1a64(bytes: &[u8]) -> u64 {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0100_0000_01b3;
    bytes.iter().fold(OFFSET, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(PRIME)
    })
}

/// The instance key of a data root: 16 hex characters.
#[must_use]
pub fn key(data_root: &Path) -> String {
    format!("{:016x}", fnv1a64(data_root.as_os_str().as_encoded_bytes()))
}

/// Eight hex characters that name the user, so users do not share a folder.
#[must_use]
pub fn user_tag(user: Option<&OsStr>) -> String {
    let user = user.unwrap_or_default();
    format!("{:016x}", fnv1a64(user.as_encoded_bytes()))
        .chars()
        .take(8)
        .collect()
}

/// The folder that holds the instance files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Location {
    /// The runtime folder of the session. The system keeps it private.
    Runtime(PathBuf),
    /// A folder under the temp folder. It must be checked before use.
    Private(PathBuf),
}

impl Location {
    /// The folder path.
    #[must_use]
    pub fn dir(&self) -> &Path {
        match self {
            Self::Runtime(dir) | Self::Private(dir) => dir,
        }
    }
}

/// Pick the folder. `xdg_runtime_dir` wins when it is set, not empty, and
/// absolute. Otherwise the folder is `temp/tarragon-<user tag>`; `user` is
/// called only then.
#[must_use]
pub fn locate(
    xdg_runtime_dir: Option<OsString>,
    temp: &Path,
    user: impl FnOnce() -> Option<OsString>,
) -> Location {
    let runtime = xdg_runtime_dir
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .filter(|path| path.is_absolute());
    match runtime {
        Some(dir) => Location::Runtime(dir),
        None => Location::Private(temp.join(format!("tarragon-{}", user_tag(user().as_deref())))),
    }
}

/// The three file paths of one instance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout {
    /// The unix socket.
    pub sock: PathBuf,
    /// The lock file.
    pub lock: PathBuf,
    /// The file with the TCP port and token.
    pub port: PathBuf,
}

impl Layout {
    /// Name the files for `key` in `location`. A shared runtime folder gets
    /// a `tarragon-` prefix, a private folder does not need one.
    #[must_use]
    pub fn new(location: &Location, key: &str) -> Self {
        let prefix = match location {
            Location::Runtime(_) => "tarragon-",
            Location::Private(_) => "",
        };
        let file = |ext: &str| location.dir().join(format!("{prefix}{key}.{ext}"));
        Self {
            sock: file("sock"),
            lock: file("lock"),
            port: file("port"),
        }
    }

    /// Whether the socket path is short enough for a unix socket.
    #[must_use]
    pub fn socket_path_fits(&self) -> bool {
        self.sock.as_os_str().as_encoded_bytes().len() <= MAX_SOCKET_PATH_BYTES
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn abs(name: &str) -> PathBuf {
        std::env::temp_dir().join(name)
    }

    #[test]
    fn fnv_matches_the_published_vectors() {
        assert_eq!(fnv1a64(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a64(b"a"), 0xaf63_dc4c_8601_ec8c);
        assert_eq!(fnv1a64(b"foobar"), 0x8594_4171_f739_67e8);
    }

    #[test]
    fn key_is_stable_and_differs_per_root() {
        let a = key(&abs("one"));
        assert_eq!(a.len(), 16);
        assert_eq!(a, key(&abs("one")));
        assert_ne!(a, key(&abs("two")));
    }

    #[test]
    fn user_tag_has_eight_hex_chars_and_accepts_no_user() {
        let tag = user_tag(Some(OsStr::new("alice")));
        assert_eq!(tag.len(), 8);
        assert!(tag.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(tag, user_tag(Some(OsStr::new("bob"))));
        assert_eq!(user_tag(None), user_tag(Some(OsStr::new(""))));
    }

    #[test]
    fn absolute_xdg_runtime_dir_is_used_directly() {
        let xdg = abs("run");
        let location = locate(Some(xdg.clone().into()), &abs("tmp"), || None);
        assert_eq!(location, Location::Runtime(xdg.clone()));
        let layout = Layout::new(&location, "0123456789abcdef");
        assert_eq!(layout.sock, xdg.join("tarragon-0123456789abcdef.sock"));
        assert_eq!(layout.lock, xdg.join("tarragon-0123456789abcdef.lock"));
    }

    #[test]
    fn unset_empty_or_relative_xdg_falls_back_to_the_temp_folder() {
        let temp = abs("tmp");
        let user = || Some(OsString::from("alice"));
        let expected = temp.join(format!("tarragon-{}", user_tag(Some(OsStr::new("alice")))));
        for xdg in [
            None,
            Some(OsString::new()),
            Some(OsString::from("relative/run")),
        ] {
            let location = locate(xdg, &temp, user);
            assert_eq!(location, Location::Private(expected.clone()));
        }
        let layout = Layout::new(&Location::Private(expected.clone()), "k");
        assert_eq!(layout.sock, expected.join("k.sock"));
        assert_eq!(layout.lock, expected.join("k.lock"));
        assert_eq!(layout.port, expected.join("k.port"));
    }

    #[test]
    fn over_long_socket_path_does_not_fit() {
        let short = Layout::new(&Location::Private(abs("s")), "k");
        assert!(short.socket_path_fits());
        let long_dir = abs(&"d".repeat(MAX_SOCKET_PATH_BYTES));
        let long = Layout::new(&Location::Private(long_dir), "k");
        assert!(!long.socket_path_fits());
    }
}
