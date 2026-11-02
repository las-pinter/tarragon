//! Jobs for files that come from stored rows, not from a scan.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use tarragon_cache::store::SourceStamp;
use tarragon_db::repository::LibraryFile;

use super::Failures;
use super::job::Job;

#[cfg(test)]
mod tests;

/// Build one job for each stored row, in row order.
///
/// The stamp comes from the row: a `stat` for every file would stall a large
/// library. `uuid_of` gets the text of a parent folder, once per folder. It
/// runs with no lock held.
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "only tests call it until the library opens")
)]
pub(super) fn library_jobs(
    files: &[LibraryFile],
    generation: u64,
    mut uuid_of: impl FnMut(&str) -> String,
    failures: &Failures,
) -> Vec<Job> {
    let mut uuids: HashMap<&str, String> = HashMap::new();
    for file in files {
        let parent = parent_of(&file.path);
        uuids.entry(parent).or_insert_with(|| uuid_of(parent));
    }
    let failures = failures
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    files
        .iter()
        .map(|file| {
            let stamp = SourceStamp {
                mtime: file.mtime,
                size: u64::try_from(file.size).unwrap_or(0),
            };
            let path = PathBuf::from(&file.path);
            let parent = parent_of(&file.path);
            let uuid = uuids[parent].clone();
            let folder = Path::new(parent).file_name().map_or_else(
                || "root".to_owned(),
                |name| name.to_string_lossy().into_owned(),
            );
            let known_failure = failures
                .get(&path)
                .filter(|(failed, _)| *failed == stamp)
                .map(|(_, error)| error.clone());
            Job {
                generation,
                path,
                stamp,
                stored: file.cached.then_some(stamp),
                folder,
                uuid,
                known_failure,
            }
        })
        .collect()
}

/// The text before the last `/`. A file at the root has the parent `/`; a
/// path without `/` has an empty parent.
fn parent_of(path: &str) -> &str {
    match path.rfind('/') {
        Some(0) => "/",
        Some(index) => &path[..index],
        None => "",
    }
}
