//! The menus of the top bar and the tab menu, which change the dock.

use egui::{Context, Ui, ViewportCommand};

use super::Shell;
use super::dock::{self, TabMenu};
use super::prefs::MenuEvent;
use super::top_bar;
use crate::layout::{Pane, is_visible, set_visible};
use crate::popup::Rows;

/// The File menu row that opens the folder dialog.
pub(super) const OPEN_FOLDER: &str = "Open Folder...";
/// The File menu row that closes the window.
pub(super) const QUIT: &str = "Quit";
/// The View menu row that puts every panel back.
pub(super) const RESET_LAYOUT: &str = "Reset Window Layout";
/// The Settings menu row that opens the dialog.
pub(super) const PREFERENCES: &str = "Preferences...";

impl Shell {
    /// Apply one choice from the tab context menu.
    pub(super) fn apply_tab_menu(&mut self, action: TabMenu) {
        match action {
            TabMenu::Hide(pane) => set_visible(&mut self.dock, pane, false),
            TabMenu::Reset => self.reset_layout(),
        }
    }

    /// Draw the File, View, and Settings menus. Open Folder, Preferences, and Quit act.
    pub(super) fn menus(&mut self, ui: &mut Ui, ctx: &Context) {
        // The scope owns the gap between the buttons, so the bar's own spacing
        // stays as the bar set it.
        ui.scope(|ui| {
            ui.spacing_mut().item_spacing.x = top_bar::MENU_GAP;
            top_bar::menu(ui, "File", |ui| {
                let rows = Rows::new(ui, &[OPEN_FOLDER, QUIT], false);
                if rows.row(ui, OPEN_FOLDER, false) {
                    self.menu_events.push(MenuEvent::OpenFolder);
                    ui.close();
                }
                if rows.row(ui, QUIT, false) {
                    ctx.send_viewport_cmd(ViewportCommand::Close);
                    // The menu must close before the window goes away.
                    ui.close();
                }
            });
            top_bar::menu(ui, "View", |ui| {
                let panes = [Pane::Library, Pane::Gallery, Pane::Preview, Pane::Log];
                let mut labels: Vec<&str> = panes.iter().map(|pane| dock::name(*pane)).collect();
                labels.push(RESET_LAYOUT);
                let rows = Rows::new(ui, &labels, true);
                for pane in panes {
                    let visible = is_visible(&self.dock, pane);
                    if rows.row(ui, dock::name(pane), visible) {
                        set_visible(&mut self.dock, pane, !visible);
                    }
                }
                ui.separator();
                if rows.row(ui, RESET_LAYOUT, false) {
                    self.reset_layout();
                }
            });
            top_bar::menu(ui, "Settings", |ui| {
                let rows = Rows::new(ui, &[PREFERENCES], false);
                if rows.row(ui, PREFERENCES, false) {
                    self.menu_events.push(MenuEvent::OpenPreferences);
                }
            });
        });
    }
}
