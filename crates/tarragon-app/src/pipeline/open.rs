//! Open a folder: scan it, store the rows, prune files that left, and queue
//! its jobs.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;

use tarragon_cache::store::SourceStamp;
use tarragon_core::entity::FileInfo;
use tarragon_db::repository::Thumbnail;
use tarragon_scan::scan::{build_pool, scan_folder_counted};

use super::job::{ColorState, Job, color_keys};
use super::{Opened, Pipeline, PipelineError, new_folder_id, prune};

impl Pipeline {
    /// Scan `folder`, store the scan rows, and queue a job for each file.
    /// It returns the new generation and the files. Jobs of the old folder are
    /// dropped.
    ///
    /// The order is: bump the generation, flush the writer, read the rows
    /// again, then prune. A render of an old open stores no row after the
    /// flush, and a row stored before it is in the rows that the prune reads.
    ///
    /// The scan and the row write run on the calling thread. Stored rows and
    /// cache files of direct-child files that are no longer in the folder are
    /// deleted; their tags stay. A scan that finds no files deletes nothing.
    ///
    /// # Errors
    ///
    /// Returns an error when the scan or a database call fails. The old
    /// folder stays current in that case.
    pub fn open_folder(&mut self, folder: &Path) -> Result<Opened, PipelineError> {
        let found = &self.counters.found;
        found.store(0, Ordering::Relaxed);
        let files = build_pool()
            .map_err(|error| PipelineError::ScanPool(error.to_string()))?
            .install(|| scan_folder_counted(folder, found))?;
        let folder_text = tarragon_core::path_key::path_key(folder);
        self.db.write_scan_group(&files)?;
        let uuid = self
            .db
            .get_or_create_folder_uuid(&folder_text, &new_folder_id())?;
        let rows: HashMap<String, _> = self
            .db
            .thumbnails_for_folder(&folder_text)?
            .into_iter()
            .map(|row| (row.path.clone(), row))
            .collect();
        // The generation moves under the failures lock, so a worker never
        // records a failure of an old job after the retried set is reset.
        // Every call that can fail is done, so an error keeps the old folder.
        let generation = {
            let _failures = self
                .failures
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let generation = self.generation.fetch_add(1, Ordering::AcqRel) + 1;
            self.retried
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .clear();
            generation
        };
        // Rows stored before the bump are committed once the flush returns, so the prune reads again.
        self.flush();
        match self.db.thumbnails_for_folder(&folder_text) {
            Ok(fresh) => {
                let fresh: HashMap<String, _> = fresh
                    .into_iter()
                    .map(|row| (row.path.clone(), row))
                    .collect();
                // An empty scan prunes nothing: a missing folder and an unmounted
                // mount point both scan as empty.
                prune::prune_missing(&self.db, &self.cache_root, folder, &fresh, &files);
            }
            Err(error) => tracing::warn!("cannot read the rows to prune {folder_text}: {error}"),
        }
        let color_state = ColorState::read(&self.db);
        let folder_name = folder.file_name().map_or_else(
            || "root".to_owned(),
            |name| name.to_string_lossy().into_owned(),
        );

        let mut failures = self
            .failures
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        // Drop failures of files that are not in this folder.
        let present: HashSet<&PathBuf> = files.iter().map(|file| &file.path).collect();
        failures.retain(|path, _| present.contains(path));
        let jobs: Vec<Job> = files
            .iter()
            .map(|file| {
                let stamp = SourceStamp {
                    mtime: file.mtime,
                    size: file.size,
                };
                let key = tarragon_core::path_key::path_key(&file.path);
                let stored = rows
                    .get(&key)
                    .filter(|row| row.thumbnail_cache_path.is_some())
                    .map(|row| SourceStamp {
                        mtime: row.mtime,
                        size: u64::try_from(row.size).unwrap_or(0),
                    });
                let known_failure = failures
                    .get(&file.path)
                    .filter(|(failed, _)| *failed == stamp)
                    .map(|(_, error)| error.clone());
                let (colors, needs_colors) = color_state.for_path(&key);
                Job {
                    generation,
                    path: file.path.clone(),
                    stamp,
                    stored,
                    folder: folder_name.clone(),
                    uuid: uuid.clone(),
                    known_failure,
                    colors,
                    needs_colors,
                }
            })
            .collect();
        drop(failures);
        let color_keys = color_keys(&jobs);
        self.jobs = jobs
            .iter()
            .map(|job| (job.path.clone(), job.clone()))
            .collect();
        self.queue.replace(jobs);
        let mut files = files;
        copy_stored_sizes(&mut files, &rows);
        Ok(Opened {
            generation,
            files,
            color_keys,
        })
    }
}

/// Copy the pixel size of each stored row whose stamp matches the scan, so
/// the grid knows it before any render. A file that changed since its row was
/// written keeps an unknown size until it renders again.
fn copy_stored_sizes(files: &mut [FileInfo], rows: &HashMap<String, Thumbnail>) {
    for file in files {
        let key = tarragon_core::path_key::path_key(&file.path);
        let Some(row) = rows.get(&key) else {
            continue;
        };
        let same = row.mtime == file.mtime && u64::try_from(row.size).ok() == Some(file.size);
        if same {
            (file.width, file.height) = (row.width, row.height);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(path: &str, size: u64, mtime: i64) -> FileInfo {
        FileInfo {
            path: PathBuf::from(path),
            size,
            mtime,
            width: 0,
            height: 0,
        }
    }

    fn row(path: &str, size: i64, mtime: i64) -> Thumbnail {
        Thumbnail {
            path: path.to_owned(),
            mtime,
            size,
            thumb_hash: None,
            width: 640,
            height: 480,
            created_at: String::new(),
            cache_uuid: None,
            thumbnail_cache_path: None,
            preview_cache_path: None,
            full_cache_path: None,
            last_accessed: 0,
        }
    }

    #[test]
    fn a_row_with_the_same_stamp_gives_its_pixel_size() {
        let mut files = [file("/images/a.png", 10, 5)];
        let rows = HashMap::from([(
            tarragon_core::path_key::path_key(Path::new("/images/a.png")),
            row("/images/a.png", 10, 5),
        )]);
        copy_stored_sizes(&mut files, &rows);
        assert_eq!((files[0].width, files[0].height), (640, 480));
    }

    #[test]
    fn a_changed_file_or_a_missing_row_keeps_the_size_unknown() {
        let mut files = [
            file("/images/newer.png", 10, 6),
            file("/images/bigger.png", 11, 5),
            file("/images/new.png", 10, 5),
        ];
        let rows = ["/images/newer.png", "/images/bigger.png"]
            .into_iter()
            .map(|path| {
                (
                    tarragon_core::path_key::path_key(Path::new(path)),
                    row(path, 10, 5),
                )
            })
            .collect();
        copy_stored_sizes(&mut files, &rows);
        assert!(files.iter().all(|f| (f.width, f.height) == (0, 0)));
    }
}
