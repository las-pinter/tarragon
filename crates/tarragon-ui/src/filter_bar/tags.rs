//! The tag filter of the filter bar: a menu button and the active tag chips.

use std::fmt::Debug;
use std::hash::Hash;

use egui::Ui;
use tarragon_core::filter_bar::FilterBar;

use super::list::{Edit, Shown, show_list};

/// Draw the "Filter Tags" button, its check menu, and one chip per active tag.
///
/// `id_salt` keys the open state of the menu.
///
/// The chips wrap and follow `tag_order()`. Reports a filter change and a menu
/// open this frame.
pub fn show_tags(
    ui: &mut Ui,
    id_salt: impl Hash + Debug,
    bar: &mut FilterBar,
    names: &[String],
) -> Shown {
    let chips: Vec<_> = bar
        .tag_order()
        .iter()
        .map(|t| (t.clone(), t.clone()))
        .collect();
    let rows: Vec<_> = names
        .iter()
        .map(|n| (n.clone(), bar.state.tags.contains(n)))
        .collect();
    let (edit, opened) = show_list(ui, id_salt, Some("Filter Tags"), &chips, &rows);
    let tag = match edit {
        Some(Edit::Chip(i)) => chips.get(i).map(|c| c.0.as_str()),
        Some(Edit::Row(i)) => names.get(i).map(String::as_str),
        None => None,
    };
    let changed = tag.is_some_and(|tag| {
        bar.toggle_tag(tag);
        true
    });
    Shown { changed, opened }
}

#[cfg(test)]
mod tests;
