//! The Library panel body: folder tree, favorites list, and the two buttons.
//!
//! The panel never touches the disk or the database. It queues events that
//! the shell turns into work.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use egui::{Button, FontFamily, Frame, Margin, Panel, RichText, ScrollArea, Ui, vec2};

use super::row::{RowAction, show_tree_row};
use super::tree::{TreeModel, TreeRow};
use super::{FavoriteRow, SidebarState};
use crate::{fonts, tokens};

mod favorites;

/// What the panel asks the shell to do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SidebarEvent {
    /// Open this folder in the gallery.
    Navigate(PathBuf),
    /// Add the open folder to the favorites.
    AddCurrent,
    /// Remove this favorite.
    Remove(PathBuf),
    /// List the subfolders of this folder, then call `set_children`.
    ///
    /// The app must answer every request, with an empty list on error.
    ListChildren(PathBuf),
    /// Save this full favorite order, then call `set_favorites`.
    ///
    /// The panel keeps its old order until then, so a failed save shows nothing.
    Reorder(Vec<PathBuf>),
}

/// The panel state and its event queue.
///
/// `SidebarState.selected` is the open folder. The selected favorite follows
/// it: the favorite equal to the open folder, else none. Remove acts on it.
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
    /// The favorite equal to the open folder, else none; Remove acts on it.
    selected_favorite: Option<PathBuf>,
    /// Events not yet taken by the shell.
    events: Vec<SidebarEvent>,
    /// Folders that a `ListChildren` event asked for and `set_children` has not answered.
    pending: HashSet<PathBuf>,
    /// The index of the dragged favorite.
    ///
    /// Held across frames for the drop, Escape, and the lost-release guard.
    dragging: Option<usize>,
    /// The height of the favorite rows in the last frame.
    ///
    /// egui sizes the favorites panel and its scroll area from the frame
    /// before and does not repaint when the content changes, so a change asks
    /// for a repaint. It also feeds `min_scrolled_height` of the favorites
    /// scroll area, which lifts the viewport above egui's 64 px default.
    favorites_height: f32,
    /// The height of the favorites panel in the last frame.
    ///
    /// The panel follows its scroll area one frame late, and a dock that
    /// grows gives no change of `favorites_height`, so a change of the panel
    /// rect also asks for a repaint. The chain ends when the rect is stable.
    favorites_panel_height: f32,
}

impl Sidebar {
    /// Replace the roots in the data and in the tree, so they cannot differ.
    pub fn set_roots(&mut self, roots: Vec<PathBuf>) {
        self.tree.set_roots(&roots);
        self.pending.retain(|path| self.tree.is_known(path));
        for root in &roots {
            ask_listing(&self.tree, &mut self.pending, &mut self.events, root);
        }
        self.state.set_roots(roots);
        self.dirty = true;
        self.reveal();
    }

    /// Store the listed children of a folder and answer its pending request.
    pub fn set_children(&mut self, path: &Path, children: Vec<PathBuf>) {
        self.tree.set_children(path, children);
        let was_pending = self.pending.remove(path);
        let below = self
            .state
            .selected()
            .is_some_and(|open| open != path && open.starts_with(path));
        if was_pending && below {
            self.reveal();
        }
        self.dirty = true;
    }

    /// Open the ancestors of the open folder, top down, so its row shows.
    ///
    /// Only roots that contain the folder count. The walk stops at the first
    /// ancestor that is not listed yet and asks for its listing; the next
    /// `set_children` continues the walk.
    fn reveal(&mut self) {
        let Some(folder) = self.state.selected().map(Path::to_path_buf) else {
            return;
        };
        let roots = self.state.roots().to_vec();
        let mut changed = false;
        for root in roots.iter().filter(|root| folder.starts_with(root)) {
            let mut chain: Vec<&Path> = folder.ancestors().skip(1).collect();
            chain.retain(|path| path.starts_with(root));
            for path in chain.into_iter().rev() {
                if !self.tree.is_known(path) {
                    break;
                }
                changed |= self.tree.set_expanded(path, true);
                if !self.tree.is_listed(path) {
                    ask_listing(&self.tree, &mut self.pending, &mut self.events, path);
                    break;
                }
            }
        }
        self.dirty |= changed;
    }

    /// Replace the favorites. The selected favorite follows the open folder.
    ///
    /// A drag in progress ends, because its index may point at another row now.
    pub fn set_favorites(&mut self, favorites: Vec<FavoriteRow>) {
        self.dragging = None;
        self.state.set_favorites(favorites);
        self.select_open_favorite();
    }

    /// The selected favorite is the one equal to the open folder, else none.
    fn select_open_favorite(&mut self) {
        self.selected_favorite = self
            .state
            .selected()
            .filter(|open| self.state.favorites().iter().any(|fav| fav.path == *open))
            .map(Path::to_path_buf);
    }

    /// Open a folder from a click and update the selected favorite.
    fn open(&mut self, path: PathBuf) {
        self.state.set_selected(Some(path));
        self.select_open_favorite();
    }

    /// The folder open in the gallery, or none.
    #[must_use]
    pub fn open_folder(&self) -> Option<&Path> {
        self.state.selected()
    }

    /// Set the folder open in the gallery. The selected favorite follows it:
    /// the favorite with that path, else none.
    pub fn set_open_folder(&mut self, folder: Option<PathBuf>) {
        self.state.set_selected(folder);
        self.select_open_favorite();
        self.reveal();
    }

    /// The favorites in list order.
    #[must_use]
    pub fn favorites(&self) -> &[FavoriteRow] {
        self.state.favorites()
    }

    /// The tree roots.
    #[must_use]
    pub fn roots(&self) -> &[PathBuf] {
        self.state.roots()
    }

    /// The visible tree rows, top down.
    #[must_use]
    pub fn tree_rows(&self) -> Vec<TreeRow> {
        self.tree.rows()
    }

    /// The favorite that Remove acts on, or none.
    #[must_use]
    pub fn selected_favorite(&self) -> Option<&Path> {
        self.selected_favorite.as_deref()
    }

    /// The folder tree, for tests; the app stores children with `set_children`.
    #[cfg(test)]
    pub fn tree_mut(&mut self) -> &mut TreeModel {
        self.dirty = true;
        &mut self.tree
    }

    /// Queue an event as if the panel had produced it. The app tests use it,
    /// because a test cannot click a row.
    pub fn push_event(&mut self, event: SidebarEvent) {
        self.events.push(event);
    }

    /// Return the queued events and empty the queue.
    pub fn take_events(&mut self) -> Vec<SidebarEvent> {
        std::mem::take(&mut self.events)
    }

    /// Draw the panel body, bottom up: the buttons, the favorites, then the tree.
    ///
    /// The favorites take their content height, at most half the dock, and
    /// scroll on their own, so they stay in view however tall the tree is.
    /// The content height of the last frame is the minimum height of the
    /// scroll area; without it egui keeps the panel at its 64 px default.
    pub fn show(&mut self, ui: &mut Ui) {
        let half = ui.available_height() / 2.0;
        Panel::bottom("tarragon-sidebar-buttons")
            .resizable(false)
            .show_separator_line(false)
            .frame(Frame::NONE.inner_margin(Margin::same(8)))
            .show(ui, |ui| self.show_buttons(ui));
        let favorites = Panel::bottom("tarragon-sidebar-favorites")
            .resizable(false)
            .show_separator_line(false)
            .frame(Frame::NONE)
            .max_size(half)
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                head(ui, "Favorites", 14.0);
                let room = (half - ui.min_rect().height()).max(0.0);
                ScrollArea::vertical()
                    .id_salt("favorites-scroll")
                    .max_height(room)
                    .min_scrolled_height(self.favorites_height.min(room))
                    .auto_shrink([false, true])
                    .show(ui, |ui| self.show_favorites(ui))
                    .content_size
                    .y
            });
        let height = favorites.inner;
        let panel_height = favorites.response.rect.height();
        if (height - self.favorites_height).abs() > 0.5
            || (panel_height - self.favorites_panel_height).abs() > 0.5
        {
            self.favorites_height = height;
            self.favorites_panel_height = panel_height;
            ui.ctx().request_repaint();
        }
        ScrollArea::vertical()
            .id_salt("tree-scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                head(ui, "Folders", 0.0);
                self.show_tree(ui);
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
                    let mut navigate = None;
                    for (index, row) in self.rows.iter().enumerate() {
                        let open = self.state.selected() == Some(row.path.as_path());
                        match show_tree_row(ui, row, open, index) {
                            RowAction::Navigate => {
                                navigate = Some(row.path.clone());
                                ui.ctx().request_repaint();
                                self.events.push(SidebarEvent::Navigate(row.path.clone()));
                            }
                            RowAction::Toggle => {
                                self.tree.toggle(&row.path);
                                if !row.expanded {
                                    ask_listing(
                                        &self.tree,
                                        &mut self.pending,
                                        &mut self.events,
                                        &row.path,
                                    );
                                }
                                self.dirty = true;
                                ui.ctx().request_repaint();
                            }
                            RowAction::None => {}
                        }
                    }
                    if let Some(path) = navigate {
                        self.open(path);
                    }
                });
            });
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
                && let Some(path) = self.selected_favorite.clone()
            {
                self.events.push(SidebarEvent::Remove(path));
            }
        });
    }
}

/// Queue a `ListChildren` for a folder that is not listed and not asked for yet.
fn ask_listing(
    tree: &TreeModel,
    pending: &mut HashSet<PathBuf>,
    events: &mut Vec<SidebarEvent>,
    path: &Path,
) {
    if tree.is_known(path) && !tree.is_listed(path) && pending.insert(path.to_path_buf()) {
        events.push(SidebarEvent::ListChildren(path.to_path_buf()));
    }
}

/// Head letter spacing, in em: 0.04em in `.side-head` of design/mockups/theme.css.
const HEAD_SPACING_EM: f32 = 0.04;

/// Draw a section head: upper case, caption size, muted, with `gap` above it.
///
/// The head uses the Medium family and a letter spacing of `HEAD_SPACING_EM`.
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
                    .family(FontFamily::Name(fonts::MEDIUM_FAMILY.into()))
                    .extra_letter_spacing(tokens::SIZE_CAPTION * HEAD_SPACING_EM)
                    .color(tokens::TEXT_MUTED),
            );
        });
}

#[cfg(test)]
mod drag_tests;
#[cfg(test)]
mod layout_tests;
#[cfg(test)]
mod tests;
