//! Path and LIKE-pattern helpers shared by the repository and the filter
//! engine.
//!
//! SQLite `LIKE` has no default escape character. Every pattern that carries
//! a user value pairs with an explicit `ESCAPE '\'` clause.

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

/// Return the LIKE pattern that matches every descendant of a folder.
///
/// SQLite `%` also matches `/`, so this includes nested paths. The folder
/// listing pairs it with [`folder_deep_like`] in a `NOT LIKE` to keep direct
/// children only. `LIKE` ignores ASCII case, so the filter engine uses
/// [`folder_range`] instead.
pub(crate) fn folder_like(folder: &str) -> String {
    let normalized = normalize_path(folder);
    let trimmed = normalized.trim_end_matches('/');
    format!("{}/%", escape_like(trimmed))
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

/// Return the LIKE pattern that matches the deeper descendants of a folder.
///
/// SQLite `%` also matches `/`. The extra `/%` tail therefore matches a path
/// with one more separator, which is a nested child and never a direct child.
pub(crate) fn folder_deep_like(folder: &str) -> String {
    let normalized = normalize_path(folder);
    let trimmed = normalized.trim_end_matches('/');
    format!("{}/%/%", escape_like(trimmed))
}
