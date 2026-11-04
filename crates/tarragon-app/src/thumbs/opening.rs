use super::{Library, Slot, Thumbs};

impl Thumbs {
    /// True while a folder or the library opens or waits to open. A purge or
    /// other work is not an open.
    #[must_use]
    pub(crate) fn is_opening(&self) -> bool {
        matches!(self.slot, Slot::Opening { .. })
            || self.pending.is_some()
            || self.library == Library::Wanted
    }
}
