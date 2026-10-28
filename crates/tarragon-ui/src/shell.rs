//! Application shell: the window frame, the menu bar, and the dock panels.
//!
//! The four panels sit in a dock, so the user can resize and move them.

use egui::{
    Button, CentralPanel, Context, FontFamily, FontId, Frame, Margin, MenuBar, Panel, RichText, Ui,
    ViewportCommand,
};
use egui_dock::{DockArea, DockState, Style};
use tarragon_core::sorting::SortMode;

use crate::gallery::Gallery;
use crate::grid::GridItem;
use crate::layout::{Pane, first_dock, from_json, is_visible, set_visible, to_json};
use crate::sidebar::panel::{Sidebar, SidebarEvent};
use crate::theme::{dock_title_style, panel_frame};
use crate::tokens;

mod dock;
use dock::TabMenu;
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

    /// The dock as JSON text, for saving between runs.
    #[must_use]
    pub fn layout_json(&self) -> String {
        to_json(&self.dock)
    }

    /// Replace the dock with a saved layout. Returns false, and keeps the
    /// current dock, when the text is not a valid layout. The caller uses
    /// the result to choose the default for the Log panel.
    pub fn restore_layout(&mut self, text: &str) -> bool {
        let Some(dock) = from_json(text) else {
            return false;
        };
        self.dock = dock;
        true
    }

    /// Apply one choice from the tab context menu.
    fn apply_tab_menu(&mut self, action: TabMenu) {
        match action {
            TabMenu::Hide(pane) => set_visible(&mut self.dock, pane, false),
            TabMenu::Reset => self.reset_layout(),
        }
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
        let mut menu = None;
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
            let title_font = FontId::new(
                tokens::SIZE_BODY,
                FontFamily::Name(crate::fonts::MEDIUM_FAMILY.into()),
            );
            dock_title_style(&mut style, ui.fonts_mut(|f| f.row_height(&title_font)));
            let mut panes = dock::Panes {
                sidebar: &mut self.sidebar,
                gallery: &mut self.gallery,
                log,
                events: &mut events,
                menu: &mut menu,
            };
            let before = tab_places(&self.dock);
            // The tab bar is the panel title: no buttons, only the right-click menu.
            DockArea::new(&mut self.dock)
                .style(style)
                .show_close_buttons(false)
                .show_add_buttons(false)
                .show_add_popup(false)
                .show_leaf_close_all_buttons(false)
                .show_leaf_collapse_buttons(false)
                .show_inside(ui, &mut panes);
            // A drop or the menu changes the tree after the panels are laid out,
            // so the new places need one more frame. Comparing the few tab
            // places is cheaper than tracking the drag state.
            if let Some(action) = menu {
                self.apply_tab_menu(action);
                ui.ctx().request_repaint();
            } else if tab_places(&self.dock) != before {
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

/// Where each panel sits in the dock: its surface, node, and tab slot.
fn tab_places(dock: &DockState<Pane>) -> Vec<(egui_dock::TabPath, Pane)> {
    dock.iter_all_tabs()
        .map(|(path, pane)| (path, *pane))
        .collect()
}

impl Default for Shell {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests;
