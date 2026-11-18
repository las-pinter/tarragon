//! Path and LIKE-pattern helpers shared by the repository and the filter
//! engine.
//!
//! `LIKE` serves the text search on the path only. It ignores ASCII case, and it has
//! no default escape character. Every pattern that carries a user value pairs
//! with an explicit `ESCAPE '\'` clause. Folder matches use [`folder_range`],
//! which is case-sensitive.

/// Return `path` with backslashes changed to forward slashes.
pub(crate) fn normalize_path(path: &str) -> String {
    path.replace('\\', "/")
}

/// Escape the LIKE wildcards `%`, `_`, and `\` so they match literally.
///
/// Escape the backslash first. It is the `ESCAPE` character, so a later
/// escape would otherwise swallow it.
pub(crate) fn escape_like(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

/// Return the half-open range `[low, high)` that holds every descendant of a
/// folder.
///
/// The compare is the BINARY collation, so it is bytewise and case-sensitive.
/// The char `0` follows `/` in ASCII. A path in the range therefore starts
/// with the folder and a `/`, and a sibling such as `Pics2` falls outside.
/// The bounds are plain values and need no LIKE escape.
pub(crate) fn folder_range(folder: &str) -> (String, String) {
    let normalized = normalize_path(folder);
    let prefix = normalized.trim_end_matches('/');
    (format!("{prefix}/"), format!("{prefix}0"))
}
