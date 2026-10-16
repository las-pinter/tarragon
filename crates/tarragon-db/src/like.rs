//! Path and LIKE-pattern helpers shared by the repository and the filter
//! engine.
//!
//! SQLite `LIKE` has no default escape character. Every pattern that carries
//! a user value pairs with an explicit `ESCAPE '\'` clause (Q21).

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

/// Return the LIKE pattern that matches a folder and its direct children.
pub(crate) fn folder_like(folder: &str) -> String {
    let normalized = normalize_path(folder);
    let trimmed = normalized.trim_end_matches('/');
    format!("{}/%", escape_like(trimmed))
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
