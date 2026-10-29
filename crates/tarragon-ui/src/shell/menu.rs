//! The menu bar and the tab menu, which change the dock.

use egui::{Button, Context, Ui, ViewportCommand};

use super::Shell;
use super::dock::{self, TabMenu};
use crate::layout::{Pane, is_visible, set_visible};

impl Shell {
    /// Apply one choice from the tab context menu.
    pub(super) fn apply_tab_menu(&mut self, action: TabMenu) {
        match action {
            TabMenu::Hide(pane) => set_visible(&mut self.dock, pane, false),
            TabMenu::Reset => self.reset_layout(),
        }
    }

    /// Draw the File, View, and Settings menus. The items are placeholders.
    pub(super) fn menu_bar(&mut self, ui: &mut Ui, ctx: &Context) {
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
