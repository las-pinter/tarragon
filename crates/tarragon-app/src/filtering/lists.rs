//! The tag and folder lists of the filter menus.

use tarragon_core::entity::TagSource;
use tarragon_db::{Database, DbError};
use tarragon_ui::gallery::Gallery;

use super::{apply_change, colors_enabled};

/// Set up the filter bar at startup: the colour row and the menu lists.
pub fn init(gallery: &mut Gallery, db: &Database) {
    gallery.set_colors_enabled(colors_enabled(db));
    refresh_lists(gallery, db);
}

/// Run the per-frame filter work. Returns true when a filter change was applied.
///
/// The lists refresh first when a menu opened, because a pruned folder chip
/// flags a change that must be applied in the same frame.
pub fn update(gallery: &mut Gallery, db: &Database) -> bool {
    if gallery.take_filter_menu_opened() {
        refresh_lists(gallery, db);
    }
    apply_change(gallery, db)
}

/// Read the user tags and the folders from the database into the filter menus.
///
/// A failed read is logged and the lists stay as they are. If either read
/// fails, neither list changes.
pub fn refresh_lists(gallery: &mut Gallery, db: &Database) {
    refresh_with(gallery, || Ok((user_tags(db)?, db.distinct_folders()?)));
}

/// Like [`refresh_lists`], with the read as a parameter.
fn refresh_with(
    gallery: &mut Gallery,
    read: impl FnOnce() -> Result<(Vec<String>, Vec<String>), DbError>,
) {
    match read() {
        Ok((tags, folders)) => gallery.set_filter_lists(tags, folders),
        Err(error) => tracing::warn!("cannot read the filter lists: {error}"),
    }
}

/// The names of the tags that the user made, in name order.
fn user_tags(db: &Database) -> Result<Vec<String>, DbError> {
    let all = db.all_tags()?;
    let user = all
        .into_iter()
        .filter(|usage| usage.tag.source == TagSource::User);
    Ok(user.map(|usage| usage.tag.name).collect())
}

#[cfg(test)]
mod tests;
