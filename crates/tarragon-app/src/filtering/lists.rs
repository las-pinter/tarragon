//! The tag and folder lists of the filter menus.

use tarragon_core::entity::TagSource;
use tarragon_db::repository::TagUsage;
use tarragon_db::{Database, DbError};
use tarragon_ui::gallery::{Gallery, TagRow};

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

/// Read the tags and the folders from the database into the filter menus and
/// the Manage tags rows.
///
/// A failed read is logged and the lists stay as they are. If either read
/// fails, nothing changes.
pub fn refresh_lists(gallery: &mut Gallery, db: &Database) {
    refresh_with(gallery, || Ok((db.all_tags()?, db.distinct_folders()?)));
}

/// Like [`refresh_lists`], with the read as a parameter.
fn refresh_with(
    gallery: &mut Gallery,
    read: impl FnOnce() -> Result<(Vec<TagUsage>, Vec<String>), DbError>,
) {
    match read() {
        Ok((all, folders)) => {
            // One read feeds both: the user tags, in name order, with their counts.
            let user: Vec<TagUsage> = all
                .into_iter()
                .filter(|usage| usage.tag.source == TagSource::User)
                .collect();
            let rows = user
                .iter()
                .map(|usage| TagRow {
                    id: usage.tag.id,
                    name: usage.tag.name.clone(),
                    count: usage.usage_count,
                })
                .collect();
            gallery.set_tag_rows(rows);
            let names = user.into_iter().map(|usage| usage.tag.name).collect();
            gallery.set_filter_lists(names, folders);
        }
        Err(error) => tracing::warn!("cannot read the filter lists: {error}"),
    }
}

#[cfg(test)]
mod tests;
