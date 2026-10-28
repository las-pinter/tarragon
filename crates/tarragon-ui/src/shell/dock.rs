//! The dock tab viewer: it draws each panel inside its tab body.

use egui::{Id, Margin, Ui, WidgetText};
use egui_dock::TabViewer;

use super::log_panel::{self, LogPanelEvents, LogView};
use super::{DOCK_GALLERY, DOCK_LIBRARY, DOCK_LOG, DOCK_PREVIEW};
use crate::gallery::Gallery;
use crate::layout::Pane;
use crate::sidebar::panel::Sidebar;
use crate::theme::{panel_frame, panel_title};
use crate::tokens;

/// What the four panels need to draw one frame.
pub(super) struct Panes<'a> {
    /// The Library panel.
    pub sidebar: &'a mut Sidebar,
    /// The gallery strip and grid.
    pub gallery: &'a mut Gallery,
    /// The lines for the Log panel.
    pub log: &'a LogView<'a>,
    /// Filled with the Log panel actions of this frame.
    pub events: &'a mut LogPanelEvents,
}

impl TabViewer for Panes<'_> {
    type Tab = Pane;

    fn id(&mut self, tab: &mut Pane) -> Id {
        Id::new(name(*tab))
    }

    fn title(&mut self, tab: &mut Pane) -> WidgetText {
        name(*tab).into()
    }

    fn ui(&mut self, ui: &mut Ui, tab: &mut Pane) {
        let (fill, margin) = match tab {
            // No inset: the title strip and the sidebar own their paddings.
            Pane::Library => (tokens::BG_SECONDARY, Margin::ZERO),
            Pane::Gallery | Pane::Preview => (tokens::BG_PRIMARY, Margin::same(8)),
            Pane::Log => (tokens::BG_SECONDARY, Margin::same(8)),
        };
        // The frame grows to the tab body, so the fill reaches every edge.
        panel_frame(fill, margin).show(ui, |ui| {
            ui.set_min_size(ui.available_size());
            match tab {
                Pane::Library => {
                    panel_title(ui, DOCK_LIBRARY);
                    self.sidebar.show(ui);
                }
                // The gallery draws no title strip: its top strip holds the tabs.
                Pane::Gallery => self.gallery.show(ui),
                Pane::Preview => panel_title(ui, DOCK_PREVIEW),
                Pane::Log => {
                    panel_title(ui, DOCK_LOG);
                    *self.events = log_panel::show(ui, self.log);
                }
            }
        });
    }

    // The body scroll area would give the panel unbounded height, so the Library
    // favorites list would never scroll. The panels scroll on their own.
    fn scroll_bars(&self, _tab: &Pane) -> [bool; 2] {
        [false, false]
    }
}

/// The panel name: the tab title and the tab id.
pub(super) fn name(pane: Pane) -> &'static str {
    match pane {
        Pane::Library => DOCK_LIBRARY,
        Pane::Gallery => DOCK_GALLERY,
        Pane::Preview => DOCK_PREVIEW,
        Pane::Log => DOCK_LOG,
    }
}

#[cfg(test)]
mod tests;
