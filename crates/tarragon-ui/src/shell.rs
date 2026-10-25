//! Application shell: the window frame, the menu bar, and the dock panels.
//!
//! The frame uses plain egui panels with fixed sizes. A docking layout lands
//! later, so the panels cannot move or resize yet.

use egui::{Button, CentralPanel, Context, Margin, MenuBar, Panel, RichText, Ui, ViewportCommand};
use tarragon_core::sorting::SortMode;

use crate::gallery::Gallery;
use crate::grid::GridItem;
use crate::theme::{panel_frame, panel_title};
use crate::tokens;

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
/// Log panel share of the free workspace height.
const LOG_HEIGHT_FRACTION: f32 = 0.25;
/// Log panel minimum height, in points. `theme.css` sets a 150 px floor.
const LOG_HEIGHT_FLOOR: f32 = 150.0;

/// The window frame. It holds the panel flags and the default dock sizes.
#[expect(
    clippy::struct_excessive_bools,
    reason = "the four dock flags mirror the four View menu toggles"
)]
pub struct Shell {
    /// Whether the Library dock shows on the left.
    library_visible: bool,
    /// Whether the Gallery panel shows in the centre.
    gallery_visible: bool,
    /// Whether the Preview dock shows on the right.
    preview_visible: bool,
    /// Whether the Log panel shows under the gallery.
    log_visible: bool,
    /// Library dock width, in points.
    sidebar_width: f32,
    /// Preview dock width, in points.
    preview_width: f32,
    /// The gallery strip and grid in the centre panel.
    gallery: Gallery,
}

impl Shell {
    /// Build the frame with the mockup defaults: the three panels shown and
    /// the log hidden.
    #[must_use]
    pub fn new() -> Self {
        Self {
            library_visible: true,
            gallery_visible: true,
            preview_visible: true,
            log_visible: false,
            sidebar_width: tokens::SIDEBAR_WIDTH,
            preview_width: tokens::PREVIEW_WIDTH,
            gallery: Gallery::new(SortMode::Natural),
        }
    }

    /// Library dock width, in points.
    #[must_use]
    pub fn sidebar_width(&self) -> f32 {
        self.sidebar_width
    }

    /// Preview dock width, in points.
    #[must_use]
    pub fn preview_width(&self) -> f32 {
        self.preview_width
    }

    /// Whether the Library dock is visible.
    #[must_use]
    pub fn library_visible(&self) -> bool {
        self.library_visible
    }

    /// Whether the Gallery panel is visible.
    #[must_use]
    pub fn gallery_visible(&self) -> bool {
        self.gallery_visible
    }

    /// Whether the Preview dock is visible.
    #[must_use]
    pub fn preview_visible(&self) -> bool {
        self.preview_visible
    }

    /// Whether the Log panel is visible.
    #[must_use]
    pub fn log_visible(&self) -> bool {
        self.log_visible
    }

    /// Replace the items that the gallery shows.
    pub fn set_items(&mut self, items: Vec<GridItem>) {
        self.gallery.set_items(items);
    }

    /// Set the folder for the gallery folder scope and forward it.
    ///
    /// The folder tab draws this folder once a scan supplies one.
    pub fn set_folder(&mut self, folder: Option<std::path::PathBuf>) {
        self.gallery.set_folder(folder);
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
        self.log_visible = !self.log_visible;
        self.log_visible
    }

    /// Restore the mockup defaults for the dock sizes and panel visibility.
    ///
    /// The gallery items and the sort mode stay as they are.
    pub fn reset_layout(&mut self) {
        self.library_visible = true;
        self.gallery_visible = true;
        self.preview_visible = true;
        self.log_visible = false;
        self.sidebar_width = tokens::SIDEBAR_WIDTH;
        self.preview_width = tokens::PREVIEW_WIDTH;
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

        if self.library_visible {
            Panel::left(format!("tarragon-{DOCK_LIBRARY}"))
                .exact_size(self.sidebar_width)
                .resizable(false)
                .frame(panel_frame(tokens::BG_SECONDARY, Margin::same(8)))
                .show(ui, |ui| {
                    panel_title(ui, DOCK_LIBRARY);
                });
        }

        if self.preview_visible {
            Panel::right(format!("tarragon-{DOCK_PREVIEW}"))
                .exact_size(self.preview_width)
                .resizable(false)
                .frame(panel_frame(tokens::BG_PRIMARY, Margin::same(8)))
                .show(ui, |ui| {
                    panel_title(ui, DOCK_PREVIEW);
                });
        }

        // A bottom panel here spans the gallery width only, as the mockup shows.
        if self.log_visible {
            let log_height = (ui.available_height() * LOG_HEIGHT_FRACTION).max(LOG_HEIGHT_FLOOR);
            Panel::bottom(format!("tarragon-{DOCK_LOG}"))
                .exact_size(log_height)
                .resizable(false)
                .frame(panel_frame(tokens::BG_SECONDARY, Margin::same(8)))
                .show(ui, |ui| {
                    panel_title(ui, DOCK_LOG);
                    events = log_panel::show(ui, log);
                });
        }

        if self.gallery_visible {
            // The gallery draws no title strip: its top strip holds the tabs.
            CentralPanel::default()
                .frame(panel_frame(tokens::BG_PRIMARY, Margin::same(8)))
                .show(ui, |ui| self.gallery.show(ui));
        }
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
            toggle_row(ui, DOCK_LIBRARY, &mut self.library_visible);
            toggle_row(ui, DOCK_GALLERY, &mut self.gallery_visible);
            toggle_row(ui, DOCK_PREVIEW, &mut self.preview_visible);
            toggle_row(ui, DOCK_LOG, &mut self.log_visible);
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

/// Draw one menu toggle row and flip the flag when the row is clicked.
fn toggle_row(ui: &mut Ui, label: &str, visible: &mut bool) {
    if ui.add(Button::new(label).selected(*visible)).clicked() {
        *visible = !*visible;
    }
}

#[cfg(test)]
mod tests;
