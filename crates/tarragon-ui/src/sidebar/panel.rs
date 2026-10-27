//! The Library panel body: folder tree, favorites list, and the two buttons.
//!
//! The panel never touches the disk or the database. It queues events that
//! the shell turns into work.

use std::path::{Path, PathBuf};

use egui::{Button, Frame, Margin, Panel, RichText, ScrollArea, Ui, vec2};

use super::favorite::{FavoriteAction, show_empty_favorites, show_favorite_row};
use super::row::{RowAction, show_tree_row};
use super::tree::{TreeModel, TreeRow};
use super::{FavoriteRow, SidebarState, display_label};
use crate::tokens;

/// What the panel asks the shell to do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SidebarEvent {
    /// Open this folder in the gallery.
    Navigate(PathBuf),
    /// Add the open folder to the favorites.
    AddCurrent,
    /// Remove this favorite.
    Remove(PathBuf),
}

/// The panel state and its event queue.
///
/// `SidebarState.selected` is the open folder. The favorite that Remove acts
/// on is kept apart, so opening a tree folder does not arm Remove.
#[derive(Debug, Default)]
pub struct Sidebar {
    /// Favorites, roots, and the open folder.
    state: SidebarState,
    /// The folder tree.
    tree: TreeModel,
    /// The visible tree rows, rebuilt only when `dirty`.
    rows: Vec<TreeRow>,
    /// Whether the tree changed since `rows` was built.
    dirty: bool,
    /// The favorite that Remove acts on.
    selected_favorite: Option<PathBuf>,
    /// Events not yet taken by the shell.
    events: Vec<SidebarEvent>,
}

impl Sidebar {
    /// Replace the roots in the data and in the tree, so they cannot differ.
    pub fn set_roots(&mut self, roots: Vec<PathBuf>) {
        self.tree.set_roots(&roots);
        self.state.set_roots(roots);
        self.dirty = true;
    }

    /// Replace the favorites; the Remove target is dropped if its row is gone.
    pub fn set_favorites(&mut self, favorites: Vec<FavoriteRow>) {
        let gone = self
            .selected_favorite
            .as_ref()
            .is_some_and(|path| !favorites.iter().any(|fav| &fav.path == path));
        if gone {
            self.selected_favorite = None;
        }
        self.state.set_favorites(favorites);
    }

    /// The folder open in the gallery, or none.
    #[must_use]
    pub fn open_folder(&self) -> Option<&Path> {
        self.state.selected()
    }

    /// Set the folder open in the gallery.
    pub fn set_open_folder(&mut self, folder: Option<PathBuf>) {
        self.state.set_selected(folder);
    }

    /// The favorite that Remove acts on, or none.
    #[must_use]
    pub fn selected_favorite(&self) -> Option<&Path> {
        self.selected_favorite.as_deref()
    }

    /// The folder tree, so the shell can store listed children.
    pub fn tree_mut(&mut self) -> &mut TreeModel {
        self.dirty = true;
        &mut self.tree
    }

    /// Return the queued events and empty the queue.
    pub fn take_events(&mut self) -> Vec<SidebarEvent> {
        std::mem::take(&mut self.events)
    }

    /// Draw the panel body: the buttons pinned at the bottom, the lists scrolling above them.
    pub fn show(&mut self, ui: &mut Ui) {
        Panel::bottom("tarragon-sidebar-buttons")
            .resizable(false)
            .show_separator_line(false)
            .frame(Frame::NONE.inner_margin(Margin::same(8)))
            .show(ui, |ui| self.show_buttons(ui));
        ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                head(ui, "Folders", 0.0);
                self.show_tree(ui);
                head(ui, "Favorites", 14.0);
                self.show_favorites(ui);
            });
    }

    /// Draw the tree rows and apply their actions.
    fn show_tree(&mut self, ui: &mut Ui) {
        if self.dirty {
            self.rows = self.tree.rows();
            self.dirty = false;
        }
        Frame::new()
            .inner_margin(Margin::symmetric(6, 2))
            .show(ui, |ui| {
                ui.push_id("tree", |ui| {
                    for (index, row) in self.rows.iter().enumerate() {
                        let open = self.state.selected() == Some(row.path.as_path());
                        match show_tree_row(ui, row, open, index) {
                            RowAction::Navigate => {
                                self.state.set_selected(Some(row.path.clone()));
                                ui.ctx().request_repaint();
                                self.events.push(SidebarEvent::Navigate(row.path.clone()));
                            }
                            RowAction::Toggle => {
                                self.tree.toggle(&row.path);
                                self.dirty = true;
                                ui.ctx().request_repaint();
                            }
                            RowAction::None => {}
                        }
                    }
                });
            });
    }

    /// Draw the favorites, or the empty text, and apply their actions.
    fn show_favorites(&mut self, ui: &mut Ui) {
        if self.state.favorites().is_empty() {
            show_empty_favorites(ui);
            return;
        }
        let mut opened = None;
        ui.push_id("favorites", |ui| {
            for (index, fav) in self.state.favorites().iter().enumerate() {
                let label = display_label(&fav.path, fav.label.as_deref(), self.state.roots());
                let chosen = self.selected_favorite.as_deref() == Some(fav.path.as_path());
                if show_favorite_row(ui, &label, chosen, fav.missing, index) == FavoriteAction::Open
                {
                    opened = Some(fav.path.clone());
                    ui.ctx().request_repaint();
                }
            }
        });
        if let Some(path) = opened {
            self.selected_favorite = Some(path.clone());
            self.state.set_selected(Some(path.clone()));
            self.events.push(SidebarEvent::Navigate(path));
        }
    }

    /// Draw the buttons; Remove needs a selected favorite.
    fn show_buttons(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = tokens::SPACING_S;
            ui.spacing_mut().button_padding = vec2(8.0, 3.0);
            let small = |text| Button::new(RichText::new(text).size(tokens::SIZE_SMALL));
            if ui.add(small("Add Current Folder")).clicked() {
                self.events.push(SidebarEvent::AddCurrent);
            }
            let remove = ui.add_enabled(self.selected_favorite.is_some(), small("Remove"));
            if remove.clicked()
                && let Some(path) = self.selected_favorite.take()
            {
                self.events.push(SidebarEvent::Remove(path));
            }
        });
    }
}

/// Draw a section head: upper case, caption size, muted, with `gap` above it.
///
/// The semibold weight and letter spacing of the mockup are not drawn.
fn head(ui: &mut Ui, text: &str, gap: f32) {
    ui.add_space(gap);
    Frame::new()
        .inner_margin(Margin {
            left: 8,
            right: 8,
            top: 8,
            bottom: 4,
        })
        .show(ui, |ui| {
            ui.label(
                RichText::new(text.to_uppercase())
                    .size(tokens::SIZE_CAPTION)
                    .color(tokens::TEXT_MUTED),
            );
        });
}

#[cfg(test)]
mod tests;
