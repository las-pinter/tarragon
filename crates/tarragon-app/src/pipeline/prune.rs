//! Remove the stored rows and cache files of files that left a folder.

use std::collections::{HashMap, HashSet};
use std::io::ErrorKind;
use std::path::Path;

use tarragon_core::entity::FileInfo;
use tarragon_db::Database;
use tarragon_db::repository::Thumbnail;

/// Delete the `thumbnails` row and the cache files of each stored row of
/// `folder` whose path is not in `files`. It returns the number of rows
/// deleted.
///
/// It sees stored rows only. A row counts only when its parent is `folder`
/// exactly: the database match ignores ASCII case, so `rows` can hold rows of
/// a sibling folder such as `pics` next to `Pics`. The `file_tags` rows stay,
/// so a file that comes back keeps its tags. A failure is logged and skipped.
/// A cache file is kept when its row could not be deleted, and when it does
/// not resolve inside `cache_root`. A cache file that is already gone is not
/// an error.
///
/// An empty scan deletes nothing: an unmounted mount point looks like an
/// empty folder.
pub(super) fn prune_missing(
    db: &Database,
    cache_root: &Path,
    folder: &Path,
    rows: &HashMap<String, Thumbnail>,
    files: &[FileInfo],
) -> usize {
    if files.is_empty() {
        return 0;
    }
    let present: HashSet<String> = files
        .iter()
        .map(|file| file.path.to_string_lossy().replace('\\', "/"))
        .collect();
    let canonical_root = std::fs::canonicalize(cache_root).ok();
    let mut pruned = 0;
    let stale = rows
        .iter()
        .filter(|(path, _)| Path::new(path.as_str()).parent() == Some(folder))
        .filter(|(path, _)| !present.contains(*path));
    for (path, row) in stale {
        if let Err(error) = db.delete_thumbnail(path) {
            tracing::warn!("cannot delete the thumbnail row for {path}: {error}");
            continue;
        }
        pruned += 1;
        for cache in [
            &row.thumbnail_cache_path,
            &row.preview_cache_path,
            &row.full_cache_path,
        ]
        .into_iter()
        .flatten()
        {
            remove_cache_file(canonical_root.as_deref(), Path::new(cache));
        }
    }
    if pruned > 0 {
        tracing::info!("removed {pruned} thumbnail rows of files that left the folder");
    }
    pruned
}

/// Remove one cache file when it resolves inside `canonical_root`. A missing
/// file is fine; other errors and a path outside the root are logged.
fn remove_cache_file(canonical_root: Option<&Path>, path: &Path) {
    let canonical = match std::fs::canonicalize(path) {
        Ok(canonical) => canonical,
        Err(error) if error.kind() == ErrorKind::NotFound => return,
        Err(error) => {
            tracing::warn!("cannot resolve the cache file {}: {error}", path.display());
            return;
        }
    };
    if !canonical_root.is_some_and(|root| canonical.starts_with(root)) {
        tracing::warn!("not removing {}: it is outside the cache", path.display());
        return;
    }
    if let Err(error) = std::fs::remove_file(path)
        && error.kind() != ErrorKind::NotFound
    {
        tracing::warn!("cannot remove the cache file {}: {error}", path.display());
    }
}
