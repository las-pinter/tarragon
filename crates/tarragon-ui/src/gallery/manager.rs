//! The "Manage tags" window hooks: rows in, changes and errors across.

use super::{Gallery, TagManagerEvent, TagRow};

impl Gallery {
    /// Replace the rows that the "Manage tags" window lists.
    pub fn set_tag_rows(&mut self, rows: Vec<TagRow>) {
        self.tag_manager.set_rows(rows);
    }

    /// Whether the "Manage tags" window is open.
    #[must_use]
    pub fn manager_open(&self) -> bool {
        self.tag_manager.is_open()
    }

    /// Open the "Manage tags" window without a click.
    #[cfg(test)]
    pub(crate) fn open_manager(&mut self) {
        self.tag_manager.open();
    }

    /// Take the tag changes the user asked for since the last call.
    pub fn take_tag_manager_events(&mut self) -> Vec<TagManagerEvent> {
        self.tag_manager.take_events()
    }

    /// Show `text` under the rename field of tag `id`. Call it when a rename
    /// was refused.
    pub fn set_tag_manager_error(&mut self, id: i64, text: impl Into<String>) {
        self.tag_manager.set_error(id, text);
    }

    /// Show `text` on the row of tag `id`. Call it when a delete failed.
    pub fn set_tag_manager_delete_error(&mut self, id: i64, text: impl Into<String>) {
        self.tag_manager.set_delete_error(id, text);
    }
}
