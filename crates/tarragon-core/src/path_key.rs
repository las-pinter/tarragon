//! The text form of a path as the database stores it.
//!
//! The database keys use `/` as the separator. On Windows a `\` is a
//! separator and cannot be part of a file name, so it becomes `/`. On other
//! systems a `\` is an ordinary file name character and stays, so `a\b.png`
//! and `a/b.png` keep two keys.

use std::path::Path;

/// The database key of `path`.
#[must_use]
pub fn path_key(path: &Path) -> String {
    normalize_key(&path.to_string_lossy())
}

/// The database key of a path given as text.
#[must_use]
pub fn normalize_key(text: &str) -> String {
    normalize_for(text, cfg!(windows))
}

/// [`normalize_key`] for the given system, so both forms are tested on each.
fn normalize_for(text: &str, windows: bool) -> String {
    if windows {
        text.replace('\\', "/")
    } else {
        text.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::normalize_for;

    #[test]
    fn windows_turns_backslashes_into_slashes() {
        assert_eq!(normalize_for(r"C:\art\a.png", true), "C:/art/a.png");
    }

    #[test]
    fn other_systems_keep_a_backslash_in_a_file_name() {
        assert_eq!(normalize_for(r"/art/a\b.png", false), r"/art/a\b.png");
    }
}
