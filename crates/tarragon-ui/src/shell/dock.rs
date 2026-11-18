//! The dock tab viewer: it draws each panel inside its tab body.

use egui::{FontFamily, Id, Margin, RichText, Ui, WidgetText};
use egui_dock::{NodePath, TabViewer};

use super::log_panel::{self, LogPanelEvents, LogView};
use super::{DOCK_GALLERY, DOCK_LIBRARY, DOCK_LOG, DOCK_PREVIEW};
use crate::fonts;
use crate::gallery::Gallery;
use crate::layout::Pane;
use crate::preview::Preview;
use crate::sidebar::panel::Sidebar;
use crate::theme::panel_frame;
use crate::tokens;

/// What the four panels need to draw one frame.
pub(super) struct Panes<'a> {
    /// The Library panel.
    pub sidebar: &'a mut Sidebar,
    /// The gallery strip and grid.
    pub gallery: &'a mut Gallery,
    /// The Preview panel.
    pub preview: &'a mut Preview,
    /// The lines for the Log panel.
    pub log: &'a LogView<'a>,
    /// Filled with the Log panel actions of this frame.
    pub events: &'a mut LogPanelEvents,
    /// Filled with the tab menu choice of this frame.
    pub menu: &'a mut Option<TabMenu>,
}

/// A choice from the tab context menu. The shell applies it after the dock
/// has finished drawing, because the dock borrows its state while it draws.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TabMenu {
    /// Hide this panel, as the View menu does.
    Hide(Pane),
    /// Restore the default window layout.
    Reset,
}

impl TabViewer for Panes<'_> {
    type Tab = Pane;

    fn id(&mut self, tab: &mut Pane) -> Id {
        Id::new(name(*tab))
    }

    // The tab bar is the panel title strip, so the text matches the mockup title.
    // The colour is set here because the theme's override text colour beats the
    // tab style colour, so the style alone would paint the titles near white.
    fn title(&mut self, tab: &mut Pane) -> WidgetText {
        RichText::new(name(*tab))
            .size(tokens::SIZE_BODY)
            .color(tokens::AMBER_ACCENT)
            .family(FontFamily::Name(fonts::MEDIUM_FAMILY.into()))
            .into()
    }

    fn context_menu(&mut self, ui: &mut Ui, tab: &mut Pane, _path: NodePath) {
        if ui.button("Hide panel").clicked() {
            *self.menu = Some(TabMenu::Hide(*tab));
            ui.close();
        }
        if ui.button("Reset Window Layout").clicked() {
            *self.menu = Some(TabMenu::Reset);
            ui.close();
        }
    }

    // The dock adds an "Eject" item for a tab that may float. Panels never float.
    fn allowed_in_windows(&self, _tab: &mut Pane) -> bool {
        false
    }

    fn ui(&mut self, ui: &mut Ui, tab: &mut Pane) {
        let (fill, margin) = match tab {
            // No inset: the sidebar owns its paddings.
            Pane::Library => (tokens::BG_SECONDARY, Margin::ZERO),
            Pane::Gallery | Pane::Preview => (tokens::BG_PRIMARY, Margin::same(8)),
            Pane::Log => (tokens::BG_SECONDARY, Margin::same(8)),
        };
        // The frame grows to the tab body, so the fill reaches every edge.
        panel_frame(fill, margin).show(ui, |ui| {
            ui.set_min_size(ui.available_size());
            match tab {
                Pane::Library => self.sidebar.show(ui),
                Pane::Gallery => self.gallery.show(ui),
                Pane::Preview => self.preview.show(ui),
                Pane::Log => *self.events = log_panel::show(ui, self.log),
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
mod preview_mosaic_tests;
#[cfg(test)]
mod preview_tests;
#[cfg(test)]
mod tests;
