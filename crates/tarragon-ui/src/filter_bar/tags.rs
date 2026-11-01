//! The tag filter of the filter bar: a menu button and the active tag chips.

use std::fmt::Debug;
use std::hash::Hash;

use egui::{Id, Ui, Vec2};
use tarragon_core::filter_bar::FilterBar;

use super::chip::show_chip;
use super::menu::{MenuResult, show_check_menu};
use crate::tokens;
use crate::widgets::small_button;

/// Draw the "Filter Tags" button, its check menu, and one chip per active tag.
///
/// `id_salt` keys the open state of the menu.
///
/// The chips wrap and follow `tag_order()`. Returns true when the tag filter
/// changed this frame.
pub fn show_tags(
    ui: &mut Ui,
    id_salt: impl Hash + Debug,
    bar: &mut FilterBar,
    names: &[String],
) -> bool {
    let open_id = Id::new(id_salt).with("open");
    let mut open = ui.data(|d| d.get_temp(open_id)).unwrap_or(false);
    let mut changed = false;
    let button = ui
        .horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = Vec2::splat(tokens::SPACING_S);
            let button = small_button(ui, "Filter Tags");
            open ^= button.clicked();
            for tag in bar.tag_order().to_vec() {
                if show_chip(ui, &tag, &tag) {
                    bar.toggle_tag(&tag);
                    changed = true;
                }
            }
            button.rect
        })
        .inner;
    if open {
        let rows: Vec<_> = names
            .iter()
            .map(|n| (n.clone(), bar.state.tags.contains(n)))
            .collect();
        match show_check_menu(ui, open_id.with("menu"), button, &rows) {
            MenuResult::Toggled(i) => {
                if let Some(name) = names.get(i) {
                    bar.toggle_tag(name);
                    changed = true;
                }
            }
            MenuResult::Closed => open = false,
            MenuResult::Open => {}
        }
    }
    ui.data_mut(|d| d.insert_temp(open_id, open));
    changed
}

#[cfg(test)]
mod tests;
