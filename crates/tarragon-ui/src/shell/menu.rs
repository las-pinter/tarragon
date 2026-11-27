//! The menus of the top bar and the tab menu, which change the dock.

use egui::{Button, Context, Ui, ViewportCommand};

use super::Shell;
use super::dock::{self, TabMenu};
use super::prefs::MenuEvent;
use super::top_bar;
use crate::layout::{Pane, is_visible, set_visible};

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
                if ui.button("Open Folder...").clicked() {
                    self.menu_events.push(MenuEvent::OpenFolder);
                    ui.close();
                }
                if ui.button("Quit").clicked() {
                    ctx.send_viewport_cmd(ViewportCommand::Close);
                    // The menu must close before the window goes away.
                    ui.close();
                }
            });
            top_bar::menu(ui, "View", |ui| {
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
            top_bar::menu(ui, "Settings", |ui| {
                if ui.button("Preferences...").clicked() {
                    self.menu_events.push(MenuEvent::OpenPreferences);
                }
            });
        });
    }
}
