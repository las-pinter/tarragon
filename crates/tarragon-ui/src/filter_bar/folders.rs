//! The folder filter of the filter bar: a menu button and the folder chips.

use std::fmt::Debug;
use std::hash::Hash;
use std::path::{Path, PathBuf};

use egui::Ui;
use tarragon_core::filter_bar::{FilterBar, short_folder_name};
use tarragon_core::filter_state::FilterScope;

use super::list::{Edit, show_list};

/// Draw the "Filter Folders" button, its check menu, and one chip per folder.
///
/// The button shows in the global scope only. The chips show in both scopes
/// and follow `folder_order()`. Chips show short names and tooltips show full
/// paths. `id_salt` keys the open state of the menu. Returns true when the
/// folder filter changed this frame.
pub fn show_folders(
    ui: &mut Ui,
    id_salt: impl Hash + Debug,
    bar: &mut FilterBar,
    folders: &[String],
    scope: FilterScope,
) -> bool {
    let chips: Vec<_> = bar
        .folder_order()
        .iter()
        .map(|p| p.to_string_lossy().into_owned())
        .map(|full| (short_folder_name(&full), full))
        .collect();
    let rows: Vec<_> = folders
        .iter()
        .map(|f| {
            (
                short_folder_name(f),
                bar.state.folders.contains(Path::new(f)),
            )
        })
        .collect();
    let button = (scope == FilterScope::Global).then_some("Filter Folders");
    let path = match show_list(ui, id_salt, button, &chips, &rows) {
        Some(Edit::Chip(i)) => bar.folder_order().get(i).cloned(),
        Some(Edit::Row(i)) => folders.get(i).map(PathBuf::from),
        None => None,
    };
    path.is_some_and(|path| {
        bar.toggle_folder(&path);
        true
    })
}

#[cfg(test)]
mod tests;
