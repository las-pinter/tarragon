//! Application shell: the window frame, the menu bar, and the dock panels.
//!
//! The four panels sit in a dock, so the user can resize and move them.

use egui::{
    Button, CentralPanel, Context, Frame, Margin, MenuBar, Panel, RichText, Ui, ViewportCommand,
};
use egui_dock::{DockArea, DockState, Style};
use tarragon_core::sorting::SortMode;

use crate::gallery::Gallery;
use crate::grid::GridItem;
use crate::layout::{Pane, first_dock, is_visible, set_visible};
use crate::sidebar::panel::{Sidebar, SidebarEvent};
use crate::theme::panel_frame;
use crate::tokens;

mod dock;
mod log_panel;
pub use log_panel::{LogLevel, LogLine, LogPanelEvents, LogSlice, LogSource, LogView};

/// The product name shown in the title bar.
pub const PRODUCT_NAME: &str = "Tarragon";

/// The Library dock name: the menu label, the panel id, and the panel title.
pub const DOCK_LIBRARY: &str = "Library";
/// The Gallery dock name: the menu label.
pub const DOCK_GALLERY: &str = "Gallery";
/// The Preview dock name: the menu label, the panel id, and the panel title.
pub const DOCK_PREVIEW: &str = "Preview";
/// The Log dock name: the menu label, the panel id, and the panel title.
pub const DOCK_LOG: &str = "Log";

/// Title bar height, in points.
const TITLEBAR_HEIGHT: f32 = 30.0;
/// Menu bar height, in points.
const MENUBAR_HEIGHT: f32 = 26.0;

/// The window frame. The dock holds which panels show and where they sit.
pub struct Shell {
    /// The gallery strip and grid in the centre panel.
    gallery: Gallery,
    /// The Library panel in the left dock.
    sidebar: Sidebar,
    /// Where each panel sits and how big it is. A hidden panel is not in it.
    dock: DockState<Pane>,
}

impl Shell {
    /// Build the frame with the mockup defaults: the three panels shown and
    /// the log hidden.
    #[must_use]
    pub fn new() -> Self {
        Self {
            gallery: Gallery::new(SortMode::Natural),
            sidebar: Sidebar::default(),
            dock: first_dock(),
        }
    }

    /// Whether the Library dock is visible.
    #[must_use]
    pub fn library_visible(&self) -> bool {
        is_visible(&self.dock, Pane::Library)
    }

    /// Whether the Gallery panel is visible.
    #[must_use]
    pub fn gallery_visible(&self) -> bool {
        is_visible(&self.dock, Pane::Gallery)
    }

    /// Whether the Preview dock is visible.
    #[must_use]
    pub fn preview_visible(&self) -> bool {
        is_visible(&self.dock, Pane::Preview)
    }

    /// Whether the Log panel is visible.
    #[must_use]
    pub fn log_visible(&self) -> bool {
        is_visible(&self.dock, Pane::Log)
    }

    /// Replace the items that the gallery shows.
    pub fn set_items(&mut self, items: Vec<GridItem>) {
        self.gallery.set_items(items);
    }

    /// Set the open folder: the gallery folder scope and the Library highlight.
    ///
    /// The folder tab draws this folder once a scan supplies one.
    pub fn set_folder(&mut self, folder: Option<std::path::PathBuf>) {
        self.sidebar.set_open_folder(folder.clone());
        self.gallery.set_folder(folder);
    }

    /// The Library panel, for the app to set the roots and the favorites.
    pub fn sidebar_mut(&mut self) -> &mut Sidebar {
        &mut self.sidebar
    }

    /// Return the Library events since the last call, and clear them.
    pub fn take_sidebar_events(&mut self) -> Vec<SidebarEvent> {
        self.sidebar.take_events()
    }

    /// The gallery, for the app to set tile textures and read the visible tiles.
    pub fn gallery_mut(&mut self) -> &mut Gallery {
        &mut self.gallery
    }

    /// Set the gallery sort mode and return the frame.
    #[must_use]
    pub fn with_sort_mode(mut self, mode: SortMode) -> Self {
        self.gallery.set_sort_mode(mode);
        self
    }

    /// Return the sort mode once after a sort change, and clear the flag.
    pub fn take_sort_changed(&mut self) -> Option<SortMode> {
        self.gallery.take_sort_changed()
    }

    /// Show or hide the Log panel. Returns the new state.
    pub fn toggle_log(&mut self) -> bool {
        self.toggle(Pane::Log)
    }

    /// Flip one panel and return its new state.
    fn toggle(&mut self, pane: Pane) -> bool {
        let show = !is_visible(&self.dock, pane);
        set_visible(&mut self.dock, pane, show);
        show
    }

    /// Restore the mockup defaults for the dock sizes and panel visibility.
    /// The Log starts hidden, as on a fresh start.
    ///
    /// The gallery items and the sort mode stay as they are.
    pub fn reset_layout(&mut self) {
        self.dock = first_dock();
    }

    /// Draw one frame of the shell with an empty log.
    pub fn show(&mut self, ui: &mut Ui) {
        self.show_with_log(
            ui,
            &LogView {
                source: &LogSlice(&[]),
                debug: false,
            },
        );
    }

    /// Draw one frame of the shell, and show `log` in the Log panel.
    ///
    /// The panel reads `log` only while it is visible, so a caller can skip
    /// building the lines when `log_visible()` is false.
    pub fn show_with_log(&mut self, ui: &mut Ui, log: &LogView<'_>) -> LogPanelEvents {
        let mut events = LogPanelEvents::default();
        let ctx = ui.ctx().clone();

        Panel::top("tarragon-titlebar")
            .exact_size(TITLEBAR_HEIGHT)
            .resizable(false)
            .frame(panel_frame(tokens::BG_SECONDARY, Margin::symmetric(10, 0)))
            .show(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.label(
                        RichText::new(PRODUCT_NAME)
                            .size(tokens::SIZE_HEADING)
                            .family(egui::FontFamily::Name(crate::fonts::BOLD_FAMILY.into())),
                    );
                });
            });

        Panel::top("tarragon-menubar")
            .exact_size(MENUBAR_HEIGHT)
            .resizable(false)
            .frame(panel_frame(tokens::BG_SECONDARY, Margin::symmetric(6, 0)))
            .show(ui, |ui| {
                MenuBar::new().ui(ui, |ui| self.menu_bar(ui, &ctx));
            });

        // The dock fills what the bars leave. It owns the panel sizes and places.
        CentralPanel::default().frame(Frame::NONE).show(ui, |ui| {
            let mut style = Style::from_egui(ui.style());
            // The panels own their insets, so the tab body adds none.
            style.tab.tab_body.inner_margin = Margin::ZERO;
            let mut panes = dock::Panes {
                sidebar: &mut self.sidebar,
                gallery: &mut self.gallery,
                log,
                events: &mut events,
            };
            let tabs = self.dock.iter_all_tabs().count();
            DockArea::new(&mut self.dock)
                .style(style)
                .show_inside(ui, &mut panes);
            // A close button changes the tree after the panels are laid out.
            if self.dock.iter_all_tabs().count() != tabs {
                ui.ctx().request_repaint();
            }
        });
        events
    }

    /// Draw the File, View, and Settings menus. The items are placeholders.
    fn menu_bar(&mut self, ui: &mut Ui, ctx: &Context) {
        ui.menu_button("File", |ui| {
            // The folder picker lands with the scan work.
            let _ = ui.button("Open Folder...");
            if ui.button("Quit").clicked() {
                ctx.send_viewport_cmd(ViewportCommand::Close);
                // The menu must close before the window goes away.
                ui.close();
            }
        });
        ui.menu_button("View", |ui| {
            for pane in [Pane::Library, Pane::Gallery, Pane::Preview, Pane::Log] {
                let visible = is_visible(&self.dock, pane);
                if ui
                    .add(Button::new(dock::name(pane)).selected(visible))
                    .clicked()
                {
                    set_visible(&mut self.dock, pane, !visible);
                }
            }
            ui.separator();
            if ui.button("Reset Window Layout").clicked() {
                self.reset_layout();
            }
        });
        ui.menu_button("Settings", |ui| {
            // The settings screens land with the settings work.
            let _ = ui.button("Preferences...");
        });
    }
}

impl Default for Shell {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests;
