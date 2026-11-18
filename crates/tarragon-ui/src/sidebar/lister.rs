//! Folder lister: the child folders of one folder, read from disk.

use std::path::{Path, PathBuf};

#[cfg(test)]
mod tests;

/// List the child folders so the tree shows a stable, readable order.
///
/// A folder that cannot be read gives an empty list, and an entry that cannot
/// be read is skipped, so one bad entry never hides the rest.
#[must_use]
pub fn list_subdirs(path: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(path) else {
        return Vec::new();
    };
    let mut named: Vec<(String, PathBuf)> = entries
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') {
                return None;
            }
            // Metadata follows a symlink, so a link to a folder counts and a
            // dangling link fails here.
            let is_dir = std::fs::metadata(entry.path()).ok()?.is_dir();
            is_dir.then(|| (name, entry.path()))
        })
        .collect();
    named.sort_by(|(left, _), (right, _)| {
        left.to_lowercase()
            .cmp(&right.to_lowercase())
            .then_with(|| left.cmp(right))
    });
    named.into_iter().map(|(_, path)| path).collect()
}
