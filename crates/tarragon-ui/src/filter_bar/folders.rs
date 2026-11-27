//! The folder filter of the filter bar: the "Folders" action and its check
//! menu.

use std::fmt::Debug;
use std::hash::Hash;
use std::path::{Path, PathBuf};

use egui::Ui;
use tarragon_core::filter_bar::{FilterBar, short_folder_name};

use super::action::FOLDERS_TEXT;
use super::list::{Edit, Shown, show_menu_action};

/// Draw the "Folders" action and its check menu of `folders`.
///
/// The rows show short names; a ticked row is a held folder. `id_salt` keys
/// the open state of the menu. The bar draws the action in the All images
/// scope only. Reports a filter change and a menu open.
pub fn show_folders(
    ui: &mut Ui,
    id_salt: impl Hash + Debug,
    bar: &mut FilterBar,
    folders: &[String],
) -> Shown {
    let rows: Vec<_> = folders
        .iter()
        .map(|f| {
            (
                short_folder_name(f),
                bar.state.folders.contains(Path::new(f)),
            )
        })
        .collect();
    let (edit, opened) = show_menu_action(ui, id_salt, FOLDERS_TEXT, &rows, None);
    let path = match edit {
        Some(Edit::Row(i)) => folders.get(i).map(PathBuf::from),
        Some(Edit::Footer) | None => None,
    };
    let changed = path.is_some_and(|path| {
        bar.toggle_folder(&path);
        true
    });
    Shown {
        changed,
        opened,
        ..Shown::default()
    }
}

#[cfg(test)]
mod tests;
