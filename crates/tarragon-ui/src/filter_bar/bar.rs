//! The filter bar: one wrapping row of swatches, chips, and text actions.
//!
//! The sizes follow the filter row of design/redesign/main-window.html.

use std::path::PathBuf;

use egui::{Align, CornerRadius, Frame, Layout, Margin, Sense, Ui, vec2};
use tarragon_core::filter_bar::{FilterBar, short_folder_name};
use tarragon_core::filter_state::FilterScope;

use super::action::{
    ACTION_HEIGHT, CLEAR_TEXT, FOLDERS_TEXT, Kind, TAGS_TEXT, action_width, text_action,
};
use super::chip::{Dot, show_chip};
use super::colors::{SWATCH_GAP, choice_of, held_swatches, show_colors};
use super::folders::show_folders;
use super::list::{Shown, close_menu};
use super::tags::show_tags;
use crate::swatch::{self, FILTER_SIZE, Swatch};
use crate::tokens;

/// The padding left and right of the bar, in points.
const PAD_X: i8 = 14;
/// The padding above and below the bar, in points.
const PAD_Y: i8 = 12;
/// The gap between the groups of the bar, in points.
pub(super) const GROUP_GAP: f32 = 16.0;
/// The gap between two chips, in points.
const CHIP_GAP: f32 = 6.0;
/// The gap between two wrapped lines, in points: room for the swatch rings.
const LINE_GAP: f32 = 8.0;
/// The divider between the swatches and the chips, in points.
const DIVIDER: egui::Vec2 = vec2(1.0, 24.0);
/// The menu key of the "Tags" action.
const TAGS_MENU: &str = "tarragon-filter-tags";
/// The menu key of the "Folders" action.
const FOLDERS_MENU: &str = "tarragon-filter-folders";
/// The width of the twelve swatches in one line, in points.
#[expect(clippy::cast_precision_loss, reason = "the swatch count is twelve")]
const SWATCH_LINE: f32 =
    Swatch::ALL.len() as f32 * FILTER_SIZE + (Swatch::ALL.len() - 1) as f32 * SWATCH_GAP;

/// One held filter as a chip.
enum Target {
    /// A colour bucket or the transparent flag.
    Color(Swatch),
    /// A user tag.
    Tag(String),
    /// A folder, shown in both scopes so a held folder never hides.
    Folder(PathBuf),
}

/// The chips of the held filters: colours in swatch order, then tags and
/// folders in the order they were added.
fn chips(bar: &FilterBar) -> Vec<Target> {
    let colors = held_swatches(bar).into_iter().map(Target::Color);
    let tags = bar.tag_order().iter().cloned().map(Target::Tag);
    let folders = bar.folder_order().iter().cloned().map(Target::Folder);
    colors.chain(tags).chain(folders).collect()
}

/// Draw the bar and report what happened.
///
/// `changed` is true when a filter changed. `opened` is true when a menu
/// opened. `tags` and `folders` fill the menus. The swatches show only with
/// `colors_enabled`. The "Folders" action shows in the All images scope only.
/// "Clear" shows only while the bar holds a filter.
///
/// The actions sit at the right end of the first line when the bar is wide
/// enough for them beside a full line of swatches; otherwise they take a line
/// of their own below the swatches and chips, again at its right end.
pub fn show(
    ui: &mut Ui,
    bar: &mut FilterBar,
    tags: &[String],
    folders: &[String],
    scope: FilterScope,
    colors_enabled: bool,
) -> Shown {
    let mut shown = Shown::default();
    Frame::new()
        .fill(tokens::BG_SECONDARY)
        .corner_radius(CornerRadius::same(tokens::RADIUS_XL))
        .inner_margin(Margin::symmetric(PAD_X, PAD_Y))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.spacing_mut().interact_size.y = ACTION_HEIGHT;
            ui.spacing_mut().item_spacing = vec2(GROUP_GAP, LINE_GAP);
            let actions = Actions::of(bar, scope);
            if !actions.0.contains(&Action::Folders) {
                close_menu(ui, FOLDERS_MENU);
            }
            let beside = !colors_enabled
                || ui.available_width() >= SWATCH_LINE + GROUP_GAP + actions.width(ui);
            if beside {
                // A sized row, so the layout does not take all the height
                // left in the parent; wrapped lines still grow it downwards.
                let row = vec2(ui.available_width(), ACTION_HEIGHT);
                let layout = Layout::right_to_left(Align::Min);
                ui.allocate_ui_with_layout(row, layout, |ui| {
                    shown |= actions.show(ui, bar, tags, folders);
                    let wrap = Layout::left_to_right(Align::Center).with_main_wrap(true);
                    ui.with_layout(wrap, |ui| shown |= show_filters(ui, bar, colors_enabled));
                });
            } else {
                ui.horizontal_wrapped(|ui| shown |= show_filters(ui, bar, colors_enabled));
                let row = vec2(ui.available_width(), ACTION_HEIGHT);
                let layout = Layout::right_to_left(Align::Center);
                ui.allocate_ui_with_layout(row, layout, |ui| {
                    shown |= actions.show(ui, bar, tags, folders);
                });
            }
        });
    shown
}

/// One text action of the bar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Action {
    /// Open the tag menu.
    Tags,
    /// Open the folder menu.
    Folders,
    /// Drop every filter of the bar.
    Clear,
}

impl Action {
    const fn text(self) -> &'static str {
        match self {
            Self::Tags => TAGS_TEXT,
            Self::Folders => FOLDERS_TEXT,
            Self::Clear => CLEAR_TEXT,
        }
    }

    const fn kind(self) -> Kind {
        match self {
            Self::Tags | Self::Folders => Kind::Menu,
            Self::Clear => Kind::Quiet,
        }
    }
}

/// The text actions that show this frame, in reading order.
struct Actions(Vec<Action>);

impl Actions {
    /// "Tags" always; "Folders" in the All images scope; "Clear" while the
    /// bar holds a filter.
    fn of(bar: &FilterBar, scope: FilterScope) -> Self {
        let mut actions = vec![Action::Tags];
        if scope == FilterScope::Global {
            actions.push(Action::Folders);
        }
        if bar.holds_any() {
            actions.push(Action::Clear);
        }
        Self(actions)
    }

    /// The width of the actions with their gaps, in points.
    #[expect(
        clippy::cast_precision_loss,
        reason = "there are three actions at most"
    )]
    fn width(&self, ui: &Ui) -> f32 {
        let texts: f32 = self
            .0
            .iter()
            .map(|a| action_width(ui, a.text(), a.kind()))
            .sum();
        texts + self.0.len().saturating_sub(1) as f32 * GROUP_GAP
    }

    /// Draw the actions into a right-to-left layout, last action first, so
    /// they read in order and end at the right edge.
    fn show(&self, ui: &mut Ui, bar: &mut FilterBar, tags: &[String], folders: &[String]) -> Shown {
        let mut shown = Shown::default();
        for &action in self.0.iter().rev() {
            match action {
                Action::Tags => shown |= show_tags(ui, TAGS_MENU, bar, tags),
                Action::Folders => {
                    shown |= show_folders(ui, FOLDERS_MENU, bar, folders);
                }
                Action::Clear => {
                    if text_action(ui, CLEAR_TEXT, Kind::Quiet).clicked() {
                        bar.clear();
                        shown.changed = true;
                    }
                }
            }
        }
        shown
    }
}

/// Draw the swatches, the divider, and the chips into a wrapping row.
fn show_filters(ui: &mut Ui, bar: &mut FilterBar, colors_enabled: bool) -> Shown {
    let mut shown = Shown::default();
    let chips = chips(bar);
    if colors_enabled {
        if let Some(choice) = show_colors(ui, bar, GROUP_GAP) {
            bar.toggle_color(choice);
            shown.changed = true;
        }
        if !chips.is_empty() {
            ui.spacing_mut().item_spacing.x = GROUP_GAP;
            let (rect, _) = ui.allocate_exact_size(DIVIDER, Sense::hover());
            ui.painter()
                .rect_filled(rect, 0.0, tokens::SURFACE_HIGHLIGHT);
        }
    }
    ui.spacing_mut().item_spacing.x = CHIP_GAP;
    for target in &chips {
        if show_target(ui, target) {
            remove(bar, target);
            shown.changed = true;
        }
    }
    shown
}

/// Draw one chip. Returns whether its x was clicked.
fn show_target(ui: &mut Ui, target: &Target) -> bool {
    match target {
        Target::Color(item) => show_chip(
            ui,
            ("color", swatch::name(*item)),
            Dot::Swatch(*item),
            &swatch::name(*item),
            None,
        ),
        Target::Tag(tag) => show_chip(ui, ("tag", tag), Dot::Fill(tokens::CORAL_STRONG), tag, None),
        Target::Folder(path) => {
            let full = path.to_string_lossy();
            show_chip(
                ui,
                ("folder", path),
                Dot::Fill(tokens::AMBER_ACCENT),
                &short_folder_name(&full),
                Some(&full),
            )
        }
    }
}

/// Drop the filter of a chip from the bar.
fn remove(bar: &mut FilterBar, target: &Target) {
    match target {
        Target::Color(item) => bar.toggle_color(choice_of(*item)),
        Target::Tag(tag) => bar.toggle_tag(tag),
        Target::Folder(path) => bar.toggle_folder(path),
    }
}

#[cfg(test)]
mod tests;
