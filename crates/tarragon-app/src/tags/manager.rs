//! Applying the Manage tags events: the writes and the refreshes after them.

use tarragon_core::tag_name::{NAME_TAKEN_TEXT, TagNameError};
use tarragon_db::{Database, DbError};
use tarragon_ui::Shell;
use tarragon_ui::gallery::{Gallery, TagManagerEvent};

use super::refresh_preview;
use crate::filtering;

/// What the window shows when a database call fails for no user reason.
const RENAME_FAILED: &str = "Cannot rename: database error";
/// The delete counterpart of [`RENAME_FAILED`].
const DELETE_FAILED: &str = "Cannot delete: database error";
/// A colour tag cannot be renamed or deleted.
const PROTECTED: &str = "Built-in tags cannot be changed";

/// The database calls the manager events need, so a test can make one fail.
trait Store {
    /// The name of the tag with this id, of any source, when there is one.
    fn tag_name(&self, id: i64) -> Result<Option<String>, DbError>;
    /// Rename the user tag and return the stored name.
    fn rename(&self, id: i64, new_name: &str) -> Result<String, DbError>;
    /// Delete the user tag. Zero means it was already gone.
    fn delete(&self, id: i64) -> Result<usize, DbError>;
}

impl Store for Database {
    fn tag_name(&self, id: i64) -> Result<Option<String>, DbError> {
        Ok(self
            .all_tags()?
            .into_iter()
            .find(|usage| usage.tag.id == id)
            .map(|usage| usage.tag.name))
    }

    fn rename(&self, id: i64, new_name: &str) -> Result<String, DbError> {
        self.rename_tag(id, new_name)
    }

    fn delete(&self, id: i64) -> Result<usize, DbError> {
        self.delete_tag(id)
    }
}

/// Take the Manage tags events and write them. Returns true when a write
/// went through, so the caller can repaint: the answer lands after the draw.
pub fn apply_manager_events(shell: &mut Shell, db: &Database) -> bool {
    let events = shell.gallery_mut().take_tag_manager_events();
    apply_events(shell, db, events)
}

/// Write `events`; after a write, show the result once for the whole batch.
fn apply_events(shell: &mut Shell, db: &Database, events: Vec<TagManagerEvent>) -> bool {
    if !write_events(db, shell.gallery_mut(), events) {
        return false;
    }
    // The chips were renamed in place first, so this read prunes only a deleted tag.
    filtering::refresh_lists(shell.gallery_mut(), db);
    refresh_preview(shell, db);
    shell.gallery_mut().refresh_filter();
    true
}

/// Write the events to the database. Returns true when any write went through.
///
/// A refused event shows its text on the row and the other events still run.
fn write_events(store: &impl Store, gallery: &mut Gallery, events: Vec<TagManagerEvent>) -> bool {
    let mut wrote = false;
    for event in events {
        wrote |= match event {
            TagManagerEvent::Rename { id, new_name } => rename(store, gallery, id, &new_name),
            TagManagerEvent::Delete { id } => delete(store, gallery, id),
        };
    }
    wrote
}

/// Rename one tag and move its active chip. Returns true when the name changed
/// or the tag was gone, so the lists must be read again.
fn rename(store: &impl Store, gallery: &mut Gallery, id: i64, new_name: &str) -> bool {
    let result = store
        .tag_name(id)
        .and_then(|old| Ok((old, store.rename(id, new_name)?)));
    match result {
        // The same name writes nothing, and a chip must not meet itself.
        Ok((Some(old), stored)) if old != stored => {
            gallery.rename_filter_tag(&old, &stored);
            true
        }
        Ok(_) => false,
        Err(error) => {
            gallery.set_tag_manager_error(id, rename_text(&error));
            // A tag that is gone leaves a dead row: count it as a write so
            // the refresh drops the row.
            matches!(error, DbError::TagNotFound(_))
        }
    }
}

/// Delete one tag. A tag that is already gone counts as done, so the lists
/// catch up with the database.
fn delete(store: &impl Store, gallery: &mut Gallery, id: i64) -> bool {
    match store.delete(id) {
        Ok(_) => true,
        Err(error) => {
            tracing::warn!("cannot delete tag {id}: {error}");
            gallery.set_tag_manager_delete_error(id, delete_text(&error));
            false
        }
    }
}

/// The text on the row for a failed delete.
fn delete_text(error: &DbError) -> &'static str {
    match error {
        DbError::ProtectedTag(_) => PROTECTED,
        _ => DELETE_FAILED,
    }
}

/// The text under the rename field for a refused rename.
fn rename_text(error: &DbError) -> String {
    match error {
        DbError::TagNameTaken(_) => NAME_TAKEN_TEXT.to_owned(),
        DbError::ReservedTagName(_) => TagNameError::Reserved.to_string(),
        DbError::InvalidTagName(reason) => reason.to_string(),
        DbError::ProtectedTag(_) => PROTECTED.to_owned(),
        DbError::TagNotFound(_) => "This tag no longer exists".to_owned(),
        other => {
            tracing::warn!("cannot rename a tag: {other}");
            RENAME_FAILED.to_owned()
        }
    }
}

#[cfg(test)]
mod tests;
