//! The filter bar: colour swatches, tags, and folders stacked in rows.

use egui::{Frame, Margin, Ui};
use tarragon_core::filter_bar::FilterBar;
use tarragon_core::filter_state::FilterScope;

use super::colors::show_colors;
use super::folders::show_folders;
use super::list::Shown;
use super::tags::show_tags;
use crate::tokens;

/// The bar padding on each side, in points. The stylesheet sets 4 px.
const BAR_PAD: i8 = 4;
/// The info bar bottom padding above the bar, in points. The stylesheet sets
/// 4 px.
const INFO_PAD: f32 = 4.0;

/// Draw the colour, tag, and folder rows and report what happened.
///
/// `changed` is true when a filter changed. `opened` is true when a menu opened.
///
/// `tags` and `folders` fill the menus. The colour row is empty when
/// `colors_enabled` is false. The folder row is empty when it has no button and
/// no chips. An empty row takes no height.
pub fn show(
    ui: &mut Ui,
    bar: &mut FilterBar,
    tags: &[String],
    folders: &[String],
    scope: FilterScope,
    colors_enabled: bool,
) -> Shown {
    let mut shown = Shown::default();
    // The bar pads 4 px on every side and puts 12 px between rows. The info
    // bar adds 4 px below itself.
    let outer = ui.spacing().item_spacing.y;
    ui.add_space((INFO_PAD - outer).max(0.0));
    Frame::new()
        .inner_margin(Margin::same(BAR_PAD))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            if colors_enabled {
                if let Some(choice) = show_colors(ui, bar, true) {
                    bar.toggle_color(choice);
                    shown.changed = true;
                }
                ui.add_space(tokens::SPACING_M);
            }
            shown |= show_tags(ui, "tarragon-filter-tags", bar, tags);
            if scope == FilterScope::Global || !bar.folder_order().is_empty() {
                ui.add_space(tokens::SPACING_M);
                shown |= show_folders(ui, "tarragon-filter-folders", bar, folders, scope);
            }
        });
    shown
}

#[cfg(test)]
mod tests;
