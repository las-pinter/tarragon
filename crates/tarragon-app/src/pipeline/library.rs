//! Jobs for files that come from stored rows, not from a scan.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;

use tarragon_cache::store::SourceStamp;
use tarragon_core::entity::FileInfo;
use tarragon_db::repository::LibraryFile;

use super::job::{ColorState, Job};
use super::{Failures, Opened, Pipeline, PipelineError, new_folder_id};

#[cfg(test)]
mod tests;

impl Pipeline {
    /// Queue a job for every stored row, so the whole library renders from
    /// the cache or from its source. It returns the new generation and the
    /// files. Jobs of the old open are dropped.
    ///
    /// The database reads run on the calling thread, so call it from a
    /// worker thread and not from the UI. The files have no pixel size.
    /// It keeps the remembered failures: a file outside the old folder may
    /// have failed before.
    ///
    /// # Errors
    ///
    /// Returns an error when a database call fails. The old jobs and the
    /// generation stay as they were in that case.
    pub fn open_library(&mut self) -> Result<Opened, PipelineError> {
        let rows = self.db.library_files()?;
        // The generation moves only after the jobs are built, so a failed
        // open does not drop the results of the folder that is still shown.
        let generation = self.generation.load(Ordering::Acquire) + 1;
        let mut parents: Vec<String> = Vec::new();
        let mut seen: HashSet<&str> = HashSet::new();
        for row in &rows {
            let parent = parent_of(&row.path);
            if seen.insert(parent) {
                parents.push(parent.to_owned());
            }
        }
        // One transaction makes every missing folder id.
        let ids = self
            .db
            .get_or_create_folder_uuids(&parents, new_folder_id)?;
        let jobs = library_jobs(
            &rows,
            generation,
            |parent| ids.get(parent).cloned().unwrap_or_default(),
            &self.failures,
            &ColorState::read(&self.db),
        );
        {
            // Under the failures lock, like `open_folder`, so a worker never
            // records a failure of an old job after the retried set is reset.
            let _failures = self
                .failures
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            self.generation.store(generation, Ordering::Release);
            self.retried
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .clear();
        }
        self.jobs = jobs
            .iter()
            .map(|job| (job.path.clone(), job.clone()))
            .collect();
        self.queue.replace(jobs);
        let files = rows
            .into_iter()
            .map(|row| FileInfo {
                path: PathBuf::from(row.path),
                size: u64::try_from(row.size).unwrap_or(0),
                mtime: row.mtime,
                width: 0,
                height: 0,
            })
            .collect();
        Ok(Opened { generation, files })
    }
}

/// Build one job for each stored row, in row order.
///
/// The stamp comes from the row: a `stat` for every file would stall a large
/// library. `uuid_of` gets the text of a parent folder, once per folder. It
/// runs with no lock held.
pub(super) fn library_jobs(
    files: &[LibraryFile],
    generation: u64,
    mut uuid_of: impl FnMut(&str) -> String,
    failures: &Failures,
    color_state: &ColorState,
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
            let (colors, needs_colors) = color_state.for_path(&file.path);
            Job {
                generation,
                path,
                stamp,
                stored: file.cached.then_some(stamp),
                folder,
                uuid,
                known_failure,
                colors,
                needs_colors,
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
