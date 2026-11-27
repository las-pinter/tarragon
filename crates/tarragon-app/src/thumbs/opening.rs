use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;

use tarragon_ui::status::Job;

use super::work::Work;
use super::{Library, Queued, Slot, Target, Thumbs};

impl Thumbs {
    /// True while a folder or the library opens or waits to open. A purge or
    /// other work is not an open.
    #[must_use]
    pub(crate) fn is_opening(&self) -> bool {
        matches!(self.slot, Slot::Opening { .. })
            || self.pending.is_some()
            || self.library == Library::Wanted
    }

    /// The folder that opens now or waits to open, for the gallery header. A
    /// library open has no folder.
    #[must_use]
    pub(crate) fn loading_folder(&self) -> Option<&Path> {
        if let Some(folder) = &self.pending {
            return Some(folder);
        }
        match &self.slot {
            Slot::Opening {
                target: Target::Folder(folder),
                ..
            } => Some(folder),
            _ => None,
        }
    }

    /// Open `folder` again, also when it is the shown folder, so the tiles
    /// follow a change on disk. A newer request replaces it.
    pub(crate) fn reload_folder(&mut self, folder: PathBuf) {
        if matches!(self.slot, Slot::Gone) {
            tracing::warn!("the pipeline is gone; {} cannot reload", folder.display());
            return;
        }
        self.pending = Some(folder);
        if self.library == Library::Failed {
            self.library = Library::Absent;
        }
        (self.repaint)();
    }

    /// Load the whole library again when the gallery shows it. Otherwise it
    /// does nothing.
    pub(crate) fn reload_library(&mut self) {
        if self.library == Library::Loaded {
            self.library = Library::Wanted;
            (self.repaint)();
        }
    }

    /// The jobs for the status bar, in start order: the open (a folder scan
    /// or the library), or the thumbnails and colors of the shown files,
    /// then a purge. The thumbnail and color counts belong to the shown
    /// files, so they wait while an open runs, and no result is taken while
    /// other work runs. An eviction is upkeep that nobody asked for, so it
    /// shows nothing.
    #[must_use]
    pub(crate) fn jobs(&self) -> Vec<Job> {
        let mut jobs = Vec::new();
        if self.is_opening() {
            jobs.push(self.open_job());
        } else if self.is_working().is_none() {
            let progress = self.feed.progress();
            let count = |value: usize| u64::try_from(value).unwrap_or(u64::MAX);
            let (done, total) = progress.thumbs;
            if done < total {
                jobs.push(Job::Thumbnails {
                    done: count(done),
                    total: count(total),
                });
            }
            let (done, total) = progress.colors;
            if done < total {
                jobs.push(Job::Colors {
                    done: count(done),
                    total: count(total),
                });
            }
        }
        if matches!(self.is_working(), Some(Work::Purge)) {
            let purge = &self.counters.purge;
            jobs.push(Job::Purge {
                done: purge.done(),
                total: purge.total(),
            });
        } else if matches!(self.queued, Some(Queued::Purge { .. })) {
            jobs.push(Job::Purge { done: 0, total: 0 });
        }
        jobs
    }

    /// The open that runs or waits: the library, or a folder scan with the
    /// files it has found. A folder that waits has found none yet.
    fn open_job(&self) -> Job {
        match &self.slot {
            Slot::Opening {
                target: Target::Library,
                ..
            } => Job::Library,
            Slot::Opening {
                target: Target::Folder(_),
                ..
            } => Job::Scan {
                found: self.counters.found.load(Ordering::Relaxed),
            },
            _ if self.pending.is_none() && self.library == Library::Wanted => Job::Library,
            _ => Job::Scan { found: 0 },
        }
    }
}
