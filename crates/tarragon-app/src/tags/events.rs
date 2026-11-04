//! Applying the Preview tag events: the writes and the refreshes after them.

use std::collections::HashMap;
use std::path::PathBuf;

use tarragon_core::entity::{Tag, TagSource};
use tarragon_core::tag_name::validate_tag_name;
use tarragon_db::{Database, DbError};
use tarragon_ui::Shell;
use tarragon_ui::preview::PreviewEvent;

use super::refresh_preview;
use crate::{colortags, filtering};

/// The database calls the tag events need, so a test can make one fail.
trait Store {
    /// The user tag of this name, made when absent.
    fn ensure_user(&self, name: &str) -> Result<Tag, DbError>;
    /// Put the tag on every path.
    fn attach(&self, paths: &[String], tag: Tag) -> Result<usize, DbError>;
    /// The id of the user tag of this name, when there is one.
    fn user_tag_id(&self, name: &str) -> Result<Option<i64>, DbError>;
    /// Take the tag off every path.
    fn detach(&self, paths: &[String], tag_id: i64) -> Result<usize, DbError>;
    /// The colour tag of this name, made when absent.
    fn ensure_auto(&self, name: &str) -> Result<Tag, DbError>;
    /// The id of the colour tag of this name, when there is one.
    fn auto_tag_id(&self, name: &str) -> Result<Option<i64>, DbError>;
    /// The tags of each path, keyed by the normalised path.
    fn file_tags(&self, paths: &[String]) -> Result<HashMap<String, Vec<Tag>>, DbError>;
}

impl Store for Database {
    fn ensure_user(&self, name: &str) -> Result<Tag, DbError> {
        self.ensure_tag(name, TagSource::User)
    }

    fn attach(&self, paths: &[String], tag: Tag) -> Result<usize, DbError> {
        self.add_tags_to_files_batch(paths, &[tag])
    }

    fn user_tag_id(&self, name: &str) -> Result<Option<i64>, DbError> {
        Ok(self
            .all_tags()?
            .into_iter()
            .find(|usage| usage.tag.source == TagSource::User && usage.tag.name == name)
            .map(|usage| usage.tag.id))
    }

    fn detach(&self, paths: &[String], tag_id: i64) -> Result<usize, DbError> {
        self.remove_tags_from_files(paths, tag_id)
    }

    fn ensure_auto(&self, name: &str) -> Result<Tag, DbError> {
        self.ensure_tag(name, TagSource::AutoColor)
    }

    fn auto_tag_id(&self, name: &str) -> Result<Option<i64>, DbError> {
        Ok(self
            .all_tags()?
            .into_iter()
            .find(|usage| usage.tag.source == TagSource::AutoColor && usage.tag.name == name)
            .map(|usage| usage.tag.id))
    }

    fn file_tags(&self, paths: &[String]) -> Result<HashMap<String, Vec<Tag>>, DbError> {
        self.tags_for_files(paths)
    }
}

/// Write the Preview `events` that change tags. Returns true when a tag write
/// succeeded, so the caller can repaint: the answer lands after the draw. The
/// card events are not tag events and are skipped.
pub fn apply_preview_events(shell: &mut Shell, db: &Database, events: Vec<PreviewEvent>) -> bool {
    apply_events(shell, db, events)
}

/// Write `events` for the selected files; after a write, show the result.
fn apply_events(shell: &mut Shell, db: &Database, events: Vec<PreviewEvent>) -> bool {
    if !write_events(db, &shell.selected_paths(), events) {
        return false;
    }
    refresh_preview(shell, db);
    // A new tag must reach the Filter Tags list, and an active tag filter must run again.
    filtering::refresh_lists(shell.gallery_mut(), db);
    shell.gallery_mut().refresh_filter();
    true
}

/// Write the events to the database. Returns true when any write succeeded.
///
/// A failed write is logged and the other events still run.
fn write_events(store: &impl Store, paths: &[PathBuf], events: Vec<PreviewEvent>) -> bool {
    let stored: Vec<String> = paths
        .iter()
        .map(|path| path.to_string_lossy().into_owned())
        .collect();
    // No file, no write: a tag made for nothing would only clutter the lists.
    if stored.is_empty() {
        return false;
    }
    let mut wrote = false;
    for event in events {
        let result = match event {
            PreviewEvent::TagApplied(name) => apply_tag(store, &name, &stored),
            PreviewEvent::TagRemoved(name) => remove_tag(store, &name, &stored),
            PreviewEvent::ColorClicked(swatch) => {
                toggle_color(store, colortags::swatch_name(swatch), &stored)
            }
            // These change no tags.
            PreviewEvent::RetryRender | PreviewEvent::OpenInEditor => Ok(false),
        };
        match result {
            Ok(done) => wrote |= done,
            Err(message) => tracing::warn!("cannot change the tags of the selection: {message}"),
        }
    }
    wrote
}

/// Put the user tag `name` on every file. Its name is checked first, because
/// the database checks only the reserved rule.
fn apply_tag(store: &impl Store, name: &str, stored: &[String]) -> Result<bool, String> {
    let name = validate_tag_name(name).map_err(|error| format!("'{name}': {error}"))?;
    let tag = store
        .ensure_user(&name)
        .map_err(|error| error.to_string())?;
    store
        .attach(stored, tag)
        .map_err(|error| error.to_string())?;
    Ok(true)
}

/// Take the user tag `name` off every selected file. An unknown name does nothing.
fn remove_tag(store: &impl Store, name: &str, stored: &[String]) -> Result<bool, String> {
    let id = store.user_tag_id(name).map_err(|error| error.to_string())?;
    let Some(id) = id else {
        return Ok(false);
    };
    store
        .detach(stored, id)
        .map_err(|error| error.to_string())?;
    Ok(true)
}

/// Toggle the colour tag `name` on the selected files.
///
/// When every file holds the colour from the `AutoColor` source, it is taken
/// off all of them. Otherwise it is put on all of them. Colour names are
/// reserved, so the tag of this name is always the `AutoColor` tag.
fn toggle_color(store: &impl Store, name: &str, stored: &[String]) -> Result<bool, String> {
    let by_file = store.file_tags(stored).map_err(|error| error.to_string())?;
    let on_all = !by_file.is_empty()
        && by_file.values().all(|tags| {
            tags.iter()
                .any(|tag| tag.source == TagSource::AutoColor && tag.name == name)
        });
    if on_all {
        let id = store.auto_tag_id(name).map_err(|error| error.to_string())?;
        let Some(id) = id else {
            return Ok(false);
        };
        store
            .detach(stored, id)
            .map_err(|error| error.to_string())?;
    } else {
        let tag = store.ensure_auto(name).map_err(|error| error.to_string())?;
        store
            .attach(stored, tag)
            .map_err(|error| error.to_string())?;
    }
    Ok(true)
}

#[cfg(test)]
mod tests;
