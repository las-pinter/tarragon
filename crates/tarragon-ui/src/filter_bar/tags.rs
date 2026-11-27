//! The tag filter of the filter bar: the "Tags" action and its check menu.

use std::fmt::Debug;
use std::hash::Hash;

use egui::Ui;
use tarragon_core::filter_bar::FilterBar;

use super::action::TAGS_TEXT;
use super::list::{Edit, Shown, show_menu_action};

/// The footer row of the tag menu, which opens the tag manager.
pub const MANAGE_TAGS_TEXT: &str = "Manage tags...";

/// Draw the "Tags" action and its check menu of `names`.
///
/// `id_salt` keys the open state of the menu. A ticked row is a held tag.
/// Reports a filter change, a menu open, and a click on the footer row.
pub fn show_tags(
    ui: &mut Ui,
    id_salt: impl Hash + Debug,
    bar: &mut FilterBar,
    names: &[String],
) -> Shown {
    let rows: Vec<_> = names
        .iter()
        .map(|n| (n.clone(), bar.state.tags.contains(n)))
        .collect();
    let (edit, opened) = show_menu_action(ui, id_salt, TAGS_TEXT, &rows, Some(MANAGE_TAGS_TEXT));
    let tag = match edit {
        Some(Edit::Row(i)) => names.get(i),
        Some(Edit::Footer) | None => None,
    };
    let changed = tag.is_some_and(|tag| {
        bar.toggle_tag(tag);
        true
    });
    Shown {
        changed,
        opened,
        manage_tags: matches!(edit, Some(Edit::Footer)),
    }
}

#[cfg(test)]
mod tests;
