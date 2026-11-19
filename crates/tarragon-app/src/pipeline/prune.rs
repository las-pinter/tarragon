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
/// It sees stored rows only. The database folder match is case-sensitive, so
/// `rows` holds rows of this folder only. A row still counts only when its
/// parent is `folder` exactly. This second guard keeps a delete from reaching
/// a row of another folder if the caller passes other rows. The `file_tags`
/// rows stay, so a file that comes back keeps its tags. The rows go in one
/// transaction; when it fails, it is logged, nothing is deleted, and every
/// cache file is kept. A cache file is also kept when it does not resolve
/// inside `cache_root`. A cache file that is already gone is not
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
    let stale: Vec<(&String, &Thumbnail)> = rows
        .iter()
        .filter(|(path, _)| Path::new(path.as_str()).parent() == Some(folder))
        .filter(|(path, _)| !present.contains(*path))
        .collect();
    if stale.is_empty() {
        return 0;
    }
    // One transaction for all rows: a folder of many deleted files would
    // otherwise wait for one commit per file.
    let paths: Vec<&str> = stale.iter().map(|(path, _)| path.as_str()).collect();
    let pruned = match db.delete_thumbnails(&paths) {
        Ok(pruned) => pruned,
        Err(error) => {
            tracing::warn!("cannot delete the thumbnail rows of files that left: {error}");
            return 0;
        }
    };
    let canonical_root = dunce::canonicalize(cache_root).ok();
    for (_, row) in stale {
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
    let canonical = match dunce::canonicalize(path) {
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
