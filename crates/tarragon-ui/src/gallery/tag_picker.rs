//! The tag picker: the menu of the Preview "+ add" button, opened at the
//! pointer for a list of files.
//!
//! The picker acts on the paths it was opened with. A change of the selection
//! while it is open does not change them. A new folder or new items leave it
//! open, as they leave the Rename and Delete dialogs.
//!
//! A right-click outside the picker closes it on the press. The tile menu
//! then opens on the release, as the click completes. A click that starts and
//! ends in one frame opens no menu, because the picker still shows.

use std::path::PathBuf;

use egui::{Area, Id, Key, Modifiers, Order, Pos2, Rect, Ui, UiBuilder, vec2};

use super::{Gallery, GridMenuEvent};
use crate::popup::{menu_frame, menu_placement};
use crate::preview::PreviewEvent;
use crate::preview::add::{AddState, show_input, show_menu};
use crate::tokens;

/// Width, in points, that the menu may take right of the pointer.
const MENU_ROOM: f32 = 320.0;

/// The id of the menu area.
pub(super) const MENU_ID: &str = "tarragon-tag-picker-menu";

/// The id of the area that holds the name input.
const INPUT_AREA_ID: &str = "tarragon-tag-picker-input-area";

/// The id of the name input.
const INPUT_ID: &str = "tarragon-tag-picker-input";

/// Width of the name input, in points.
const INPUT_WIDTH: f32 = 220.0;

/// Height that the placement reserves for the name input and its note.
const INPUT_HEIGHT: f32 = 64.0;

/// The state of the open picker.
#[derive(Debug)]
pub(super) struct TagPicker {
    /// Where the picker opened, in points.
    at: Pos2,
    /// The files that get the tag, as they were when the picker opened.
    paths: Vec<PathBuf>,
    /// Which part of the picker shows.
    state: AddState,
}

impl TagPicker {
    /// The 1 by 1 rect that the menu opens from.
    fn anchor(&self) -> Rect {
        Rect::from_min_size(self.at, vec2(1.0, 1.0))
    }
}

/// Draw the name input in a menu frame that opens from `anchor`.
fn show_input_area(ui: &Ui, anchor: Rect, state: &mut AddState, events: &mut Vec<PreviewEvent>) {
    let bounds = ui.ctx().content_rect().shrink(tokens::MENU_EDGE_MARGIN);
    let (pos, pivot) = menu_placement(anchor, INPUT_HEIGHT, bounds);
    Area::new(Id::new(INPUT_AREA_ID))
        .order(Order::Foreground)
        .fixed_pos(pos)
        .pivot(pivot)
        .constrain_to(bounds)
        .show(ui.ctx(), |ui| {
            menu_frame().show(ui, |ui| {
                ui.set_width(INPUT_WIDTH);
                show_input(ui, Id::new(INPUT_ID), state, events);
            });
        });
}

impl Gallery {
    /// Open the tag picker at `at` for `paths`. A tile menu that is open
    /// closes.
    ///
    /// Nothing opens for an empty list, or while a picker, a Rename dialog, or
    /// a Delete dialog is open.
    pub fn open_tag_picker(&mut self, at: Pos2, paths: Vec<PathBuf>) {
        let busy = self.tag_picker.is_some() || self.rename.is_some() || self.delete.is_some();
        if paths.is_empty() || busy {
            return;
        }
        self.close_tile_menu();
        self.tag_picker = Some(TagPicker {
            at,
            paths,
            state: AddState::Menu,
        });
    }

    /// Whether the tag picker is open.
    #[must_use]
    pub fn tag_picker_open(&self) -> bool {
        self.tag_picker.is_some()
    }

    /// Draw the tag picker when it is open. A tag choice, or a valid new name,
    /// queues [`GridMenuEvent::TagPaths`] and closes the picker. Escape or a
    /// press outside closes it with no event.
    pub(super) fn show_tag_picker(&mut self, ui: &mut Ui) {
        let Some(mut picker) = self.tag_picker.take() else {
            return;
        };
        // Consumed here, so no other handler sees the same Escape.
        if ui.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Escape)) {
            return;
        }
        // The menu caps its width at the room right of its corner. Give it the
        // room of a wide menu whatever the pane edge, then the area shifts it
        // inside the window.
        let room =
            Rect::from_min_size(picker.at, vec2(MENU_ROOM, ui.ctx().content_rect().height()));
        let ui = ui.new_child(UiBuilder::new().max_rect(room));
        let anchor = picker.anchor();
        let mut events = Vec::new();
        match picker.state {
            AddState::Closed => return,
            AddState::Menu => {
                let choices: Vec<&str> = self.tags.iter().map(String::as_str).collect();
                let id = Id::new(MENU_ID);
                show_menu(&ui, id, anchor, &choices, &mut picker.state, &mut events);
            }
            AddState::Input { .. } => {
                show_input_area(&ui, anchor, &mut picker.state, &mut events);
            }
        }
        for event in events {
            if let PreviewEvent::TagApplied(tag) = event {
                let paths = picker.paths.clone();
                self.tile_menu
                    .push_event(GridMenuEvent::TagPaths { paths, tag });
            }
        }
        if picker.state != AddState::Closed {
            self.tag_picker = Some(picker);
        }
    }
}
