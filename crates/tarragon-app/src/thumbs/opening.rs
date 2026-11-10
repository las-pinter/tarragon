use super::work::Work;
use super::{Library, Queued, Slot, Thumbs};

impl Thumbs {
    /// True while a folder or the library opens or waits to open. A purge or
    /// other work is not an open.
    #[must_use]
    pub(crate) fn is_opening(&self) -> bool {
        matches!(self.slot, Slot::Opening { .. })
            || self.pending.is_some()
            || self.library == Library::Wanted
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
