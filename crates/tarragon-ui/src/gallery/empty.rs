//! The rule that picks which empty card the gallery shows, and the card.

mod icon;

use egui::{Align, Label, RichText, Ui, UiBuilder};
use tarragon_core::format::SUPPORTED_EXTENSIONS;

use self::icon::folder_icon;
use super::{Gallery, Scope};
use crate::tokens;
use crate::widgets::action_button;

/// The width at which the hint wraps.
const HINT_WIDTH: f32 = 440.0;
/// The size of the big line.
const BIG_SIZE: f32 = 15.0;

/// Which empty card the gallery shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EmptyKind {
    /// No folder is open and there is nothing to list.
    NoFolder,
    /// The open folder holds no image.
    EmptyFolder,
    /// A filter hides every image.
    NoResult,
}

impl Gallery {
    /// Set whether a scan or a query is still running.
    ///
    /// While it runs, no empty card shows.
    pub fn set_loading(&mut self, loading: bool) {
        self.loading = loading;
    }

    /// The empty card to show, or none when the grid has cells or work runs.
    #[must_use]
    pub fn empty_kind(&self) -> Option<EmptyKind> {
        if !self.visible.is_empty() || self.loading {
            return None;
        }
        if self.filter_active() {
            return Some(EmptyKind::NoResult);
        }
        let no_source = match self.scope {
            Scope::Folder => self.folder.is_none(),
            Scope::All => self.items.is_empty(),
        };
        Some(if no_source {
            EmptyKind::NoFolder
        } else {
            EmptyKind::EmptyFolder
        })
    }

    /// Return whether the Open Folder button was clicked since the last call.
    ///
    /// The flag clears when read.
    pub fn take_open_folder_request(&mut self) -> bool {
        std::mem::take(&mut self.open_folder_requested)
    }

    /// Fill the grid area with the empty card for `kind`.
    pub(super) fn show_empty(&mut self, ui: &mut Ui, kind: EmptyKind) {
        let area = ui.available_size();
        // Measure the column first, so the real pass can centre it. A child
        // ui is used because a scope would move the parent cursor down.
        let mut probe = ui.new_child(
            UiBuilder::new()
                .max_rect(ui.available_rect_before_wrap())
                .sizing_pass()
                .invisible(),
        );
        column(&mut probe, kind);
        let height = probe.min_rect().height();
        let clicked = ui
            .allocate_ui(area, |ui| {
                ui.add_space(((area.y - height) / 2.0).max(0.0));
                column(ui, kind)
            })
            .inner;
        self.open_folder_requested |= clicked;
    }

    /// Return whether the search, a chip, or the database result filters.
    fn filter_active(&self) -> bool {
        !self.bar.state.is_empty(self.filter_scope()) || self.filter_result.is_some()
    }
}

impl EmptyKind {
    /// The big line, the hint, and the button label if the card has one.
    fn text(self) -> (&'static str, String, Option<&'static str>) {
        match self {
            Self::NoFolder => (
                "Open a folder to begin",
                "Use File - Open Folder, or click a folder in the Library tree. \
                 Tarragon scans the folder, writes the index, and renders thumbnails."
                    .to_owned(),
                Some("Open Folder..."),
            ),
            Self::EmptyFolder => {
                let names: Vec<String> = SUPPORTED_EXTENSIONS
                    .iter()
                    .map(|extension| extension.trim_start_matches('.').to_uppercase())
                    .collect();
                (
                    "No supported images in this folder",
                    format!(
                        "This folder has no file with a supported extension. Supported: {}.",
                        names.join(", ")
                    ),
                    Some("Open Another Folder..."),
                )
            }
            Self::NoResult => (
                "No images match",
                "Change or clear the search and the filters to see more images.".to_owned(),
                None,
            ),
        }
    }
}

/// Draw the centred column. Return whether the button was clicked.
fn column(ui: &mut Ui, kind: EmptyKind) -> bool {
    let (big, hint, button) = kind.text();
    ui.vertical_centered(|ui| {
        ui.spacing_mut().item_spacing.y = tokens::SPACING_M;
        folder_icon(ui);
        ui.label(
            RichText::new(big)
                .size(BIG_SIZE)
                .color(tokens::TEXT_SECONDARY),
        );
        ui.scope(|ui| {
            ui.set_max_width(HINT_WIDTH.min(ui.available_width()));
            ui.add(
                Label::new(
                    RichText::new(hint)
                        .size(tokens::SIZE_CAPTION)
                        .color(tokens::TEXT_MUTED),
                )
                .halign(Align::Center),
            );
        });
        button.is_some_and(|label| action_button(ui, label).clicked())
    })
    .inner
}
