use std::path::{Path, PathBuf};

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

    /// The folder that opens now or waits to open, for the info bar. A library
    /// open has no folder.
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

    /// True while an open or a purge runs or waits. The gallery shows its busy
    /// strip for this, so the strip stays until the last request ends. An
    /// eviction is upkeep that nobody asked for, so it does not count.
    #[must_use]
    pub(crate) fn is_busy(&self) -> bool {
        let purging = matches!(self.is_working(), Some(Work::Purge))
            || matches!(self.queued, Some(Queued::Purge { .. }));
        self.is_opening() || purging
    }
}
