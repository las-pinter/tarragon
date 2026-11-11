//! The tile context menu: the rows it lists and the events they raise.

use std::path::PathBuf;

use egui::{Align2, Area, FontId, Id, Key, Modifiers, Order, Pos2, Sense, Ui, vec2};

use super::{Gallery, Scope};
use crate::grid::CellClick;
use crate::popup::{menu_frame, row_frame, text_width};
use crate::tokens;
use rows::{Row, empty_groups, event, groups};

mod grid_key;
mod rows;

/// The id of the menu area.
const MENU_ID: &str = "tarragon-tile-menu";

/// The height of the line between two groups: a 1 point rule with 4 points of
/// margin either side.
const SEPARATOR: f32 = tokens::STROKE_WIDTH + 2.0 * tokens::SPACING_XS;

/// What the user chose in the tile menu. The gallery does no work for a
/// choice. The app takes the event and acts on it.
///
/// The paths are the selected files in the order the grid shows them.
#[derive(Clone, Debug, PartialEq)]
pub enum GridMenuEvent {
    /// Open the files in the editor.
    Open(Vec<PathBuf>),
    /// Show the file in the file manager.
    ShowInFileManager(PathBuf),
    /// Render the thumbnails of the files again.
    RegenerateThumbnails(Vec<PathBuf>),
    /// Ask for a tag to add to the files. `pos` is where the menu opened.
    AddTag {
        /// Where the menu opened, in points.
        pos: Pos2,
        /// The files that get the tag.
        paths: Vec<PathBuf>,
    },
    /// Copy the path of the file as text.
    CopyPath(PathBuf),
    /// Copy the files to the clipboard.
    Copy(Vec<PathBuf>),
    /// Paste the clipboard files into the open folder. The handler reads the
    /// folder from [`Gallery::folder`].
    Paste,
    /// Make a copy of each file next to it.
    Duplicate(Vec<PathBuf>),
    /// Ask for a new name for the file.
    Rename(PathBuf),
    /// Give the file a new name. `new_stem` is the name without the extension,
    /// which stays as it is.
    RenameTo {
        /// The file to rename.
        path: PathBuf,
        /// The new stem, which passed the name rules.
        new_stem: String,
    },
    /// Move the files to the trash.
    Delete(Vec<PathBuf>),
    /// Delete the files with no way back.
    DeletePermanently(Vec<PathBuf>),
    /// The user confirmed a delete in the dialog. The files are the ones the
    /// dialog showed. `permanent` is set to delete with no way back.
    ConfirmedDelete {
        /// The files to delete.
        paths: Vec<PathBuf>,
        /// Whether to delete for good instead of moving to the trash.
        permanent: bool,
    },
}

/// Whether the menu is open, and the choices that wait for the app.
#[derive(Debug, Default)]
pub(super) struct TileMenu {
    /// Where the menu opened, or none while it is closed.
    open_at: Option<Pos2>,
    /// The selection revision when the menu opened.
    revision: u64,
    /// Whether the open menu is the empty-space menu, not the tile menu.
    empty_space: bool,
    /// Whether the open menu came from the empty-folder card.
    pub(super) from_card: bool,
    /// The choices since the last take.
    events: Vec<GridMenuEvent>,
}

impl TileMenu {
    /// Queue a choice for the app.
    pub(super) fn push_event(&mut self, event: GridMenuEvent) {
        self.events.push(event);
    }
}

/// Draw one row `width` wide. Returns whether an enabled row was clicked.
fn draw_row(ui: &mut Ui, row: &Row, width: f32) -> bool {
    let (rect, clicked, color) = if row.enabled {
        let (rect, response, color) = row_frame(ui, width);
        (rect, response.clicked(), color)
    } else {
        let size = vec2(width, tokens::MENU_ROW_HEIGHT);
        let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
        (rect, false, tokens::TEXT_MUTED)
    };
    let font = FontId::proportional(tokens::SIZE_BODY);
    let pad = vec2(tokens::MENU_ROW_PAD_X, 0.0);
    ui.painter().text(
        rect.left_center() + pad,
        Align2::LEFT_CENTER,
        &row.label,
        font.clone(),
        color,
    );
    ui.painter().text(
        rect.right_center() - pad,
        Align2::RIGHT_CENTER,
        row.shortcut,
        font,
        tokens::TEXT_MUTED,
    );
    clicked
}

impl Gallery {
    /// Open the tile menu at the pointer. A right-click on an unselected tile
    /// selects only that tile first. On a selected tile the selection stays.
    pub(super) fn open_tile_menu(&mut self, ui: &Ui, click: CellClick) {
        if !self.selection.contains(click.index) {
            self.selection.select_only(click.index);
            self.sync_selection_revision();
        }
        let pos = ui
            .input(|input| input.pointer.interact_pos())
            .unwrap_or_else(|| ui.max_rect().center());
        self.tile_menu.open_at = Some(pos);
        self.tile_menu.empty_space = false;
        self.tile_menu.from_card = false;
        self.tile_menu.revision = self.selection_revision;
    }

    /// Open the empty-space menu at the pointer. It opens only while a folder
    /// is open. It replaces the tile menu, so one menu shows at a time.
    /// Returns whether the menu opened.
    pub(super) fn open_empty_space_menu(&mut self, ui: &Ui) -> bool {
        if self.folder.is_none() {
            return false;
        }
        let pos = ui
            .input(|input| input.pointer.interact_pos())
            .unwrap_or_else(|| ui.max_rect().center());
        self.tile_menu.open_at = Some(pos);
        self.tile_menu.empty_space = true;
        self.tile_menu.from_card = false;
        self.tile_menu.revision = self.selection_revision;
        true
    }

    /// Close the tile menu with no event.
    pub(super) fn close_tile_menu(&mut self) {
        self.tile_menu.open_at = None;
    }

    /// Draw the tile menu while it is open. A click on an enabled row queues its
    /// event and closes the menu. Escape or a press outside closes it with no
    /// event. A change of the selected files under the open menu closes it
    /// too, so a click never acts on files the user did not see.
    pub(super) fn show_tile_menu(&mut self, ui: &Ui) {
        let Some(pos) = self.tile_menu.open_at else {
            return;
        };
        let empty_space = self.tile_menu.empty_space;
        let count = self.selection.len();
        let no_target = if empty_space {
            // The folder can change while the menu is open.
            self.folder.is_none()
        } else {
            count == 0
        };
        // A card menu ends when the card goes away.
        let card_gone = self.tile_menu.from_card && self.empty_kind().is_none();
        if no_target || card_gone || self.selection_revision != self.tile_menu.revision {
            self.close_tile_menu();
            return;
        }
        let paste = self.scope == Scope::Folder;
        let groups = if empty_space {
            empty_groups(paste)
        } else {
            Vec::from(groups(count, paste))
        };
        let bounds = ui.ctx().content_rect().shrink(tokens::MENU_EDGE_MARGIN);
        let area = Area::new(Id::new(MENU_ID))
            .order(Order::Foreground)
            .fixed_pos(pos)
            .constrain_to(bounds)
            .show(ui.ctx(), |ui| {
                menu_frame()
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = 0.0;
                        let gap = tokens::SPACING_L;
                        let widest = groups
                            .iter()
                            .flatten()
                            .map(|row| {
                                text_width(ui, &row.label) + gap + text_width(ui, row.shortcut)
                            })
                            .fold(0.0, f32::max);
                        let width = widest + 2.0 * tokens::MENU_ROW_PAD_X;
                        let mut chosen = None;
                        for (number, group) in groups.iter().enumerate() {
                            if number > 0 {
                                ui.add(egui::Separator::default().spacing(SEPARATOR));
                            }
                            for row in group {
                                if draw_row(ui, row, width) {
                                    chosen = Some(row.kind);
                                }
                            }
                        }
                        chosen
                    })
                    .inner
            });
        if let Some(kind) = area.inner {
            self.close_tile_menu();
            let paths = if empty_space {
                Vec::new()
            } else {
                self.selected_paths()
            };
            if let Some(event) = event(kind, paths, pos) {
                self.tile_menu.events.push(event);
            }
            return;
        }
        let escape = ui.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Escape));
        let outside = ui.input(|input| {
            input.pointer.any_pressed()
                && input
                    .pointer
                    .interact_pos()
                    .is_some_and(|at| !area.response.rect.contains(at))
        });
        if escape || outside {
            self.close_tile_menu();
        }
    }

    /// Return the choices the user made in the tile menu, and clear them.
    pub fn take_menu_events(&mut self) -> Vec<GridMenuEvent> {
        std::mem::take(&mut self.tile_menu.events)
    }
}
