//! The colour tags setting, kept as a flag on the Preview and the Gallery.
//!
//! Only the Preferences save writes the setting, so the app reads it at start
//! and at each save. A refresh then reads no setting from the database.

use tarragon_db::Database;
use tarragon_ui::Shell;

use crate::filtering::colors_enabled;

/// Show or hide the colour squares of the Preview and the colour row of the
/// Gallery.
pub fn apply(shell: &mut Shell, enabled: bool) {
    shell.preview_mut().set_colors_enabled(enabled);
    shell.gallery_mut().set_colors_enabled(enabled);
}

/// Read the setting once and apply it.
pub fn load(shell: &mut Shell, db: &Database) {
    apply(shell, colors_enabled(db));
}
