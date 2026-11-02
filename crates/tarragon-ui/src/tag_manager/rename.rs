//! The rows of the "Manage tags" window and the inline rename of one row.

use egui::text_edit::TextEditState;
use egui::{Key, Label, Layout, Modifiers, RichText, ScrollArea, Sense, TextEdit, UiBuilder, vec2};
use tarragon_core::tag_name::validate_tag_name;

use super::{TagManagerEvent, TagRow, delete};
use crate::tokens;

/// The most rows the list shows before it scrolls.
const MAX_VISIBLE_ROWS: f32 = 8.0;

/// The row that is being renamed.
#[derive(Debug)]
pub(super) struct Edit {
    id: i64,
    text: String,
    error: Option<String>,
    /// Set until the first frame, which drops the old cursor and undo history.
    fresh: bool,
}

impl Edit {
    /// Start an edit on `row`, filled with its name.
    pub(super) fn new(row: &TagRow) -> Self {
        Self {
            id: row.id,
            text: row.name.clone(),
            error: None,
            fresh: true,
        }
    }

    /// Start an edit on `row` filled with `text`, showing `error` under the
    /// field.
    pub(super) fn with_error(row: &TagRow, text: String, error: String) -> Self {
        Self {
            text,
            error: Some(error),
            ..Self::new(row)
        }
    }

    /// The id of the row under edit.
    pub(super) fn id(&self) -> i64 {
        self.id
    }
}

/// What Enter does with the typed name.
enum Outcome {
    /// The name did not change.
    Same,
    /// A valid new name.
    Rename(String),
    /// The text to show under the field.
    Refused(String),
}

/// Judge the typed name against the row and the other rows.
fn judge(rows: &[TagRow], row: &TagRow, text: &str) -> Outcome {
    if text.trim() == row.name {
        return Outcome::Same;
    }
    match validate_tag_name(text) {
        Err(error) => Outcome::Refused(error.to_string()),
        Ok(name) if rows.iter().any(|r| r.id != row.id && r.name == name) => {
            Outcome::Refused("name already used".to_owned())
        }
        Ok(name) => Outcome::Rename(name),
    }
}

/// The text after the name: "N images", or "1 image".
pub(super) fn count_text(count: u32) -> String {
    if count == 1 {
        "1 image".to_owned()
    } else {
        format!("{count} images")
    }
}

/// What a click on a plain row asked for.
enum Click {
    None,
    Rename,
    Delete,
}

/// Draw the rows in a scroll area, or the empty text. One row at a time is in
/// edit or in confirm; starting one ends the other.
pub(super) fn list(
    ui: &mut egui::Ui,
    rows: &[TagRow],
    edit: &mut Option<Edit>,
    confirm: &mut Option<i64>,
    events: &mut Vec<TagManagerEvent>,
) {
    if rows.is_empty() {
        ui.label(RichText::new("No user tags yet").color(tokens::TEXT_MUTED));
        return;
    }
    ScrollArea::vertical()
        .max_height(tokens::MENU_ROW_HEIGHT * MAX_VISIBLE_ROWS)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            for row in rows {
                if edit.as_ref().is_some_and(|e| e.id() == row.id) {
                    edit_row(ui, rows, row, edit, events);
                    if edit.is_some() {
                        continue;
                    }
                }
                if *confirm == Some(row.id) {
                    delete::confirm_row(ui, row, confirm, events);
                    if confirm.is_some() {
                        continue;
                    }
                }
                match plain_row(ui, row) {
                    Click::Rename => {
                        *edit = Some(Edit::new(row));
                        *confirm = None;
                    }
                    Click::Delete => {
                        *confirm = Some(row.id);
                        *edit = None;
                    }
                    Click::None => {}
                }
            }
        });
}

/// Draw one row; return which button was clicked. The buttons are laid out
/// first from the right, then the count, then the name takes the rest.
fn plain_row(ui: &mut egui::Ui, row: &TagRow) -> Click {
    let (row_rect, _) = ui.allocate_exact_size(
        vec2(ui.available_width(), tokens::MENU_ROW_HEIGHT),
        Sense::hover(),
    );
    let right_to_left = Layout::right_to_left(egui::Align::Center);
    let (click, rest) = ui
        .scope_builder(
            UiBuilder::new().max_rect(row_rect).layout(right_to_left),
            |ui| {
                let click = if ui.small_button("Delete").clicked() {
                    Click::Delete
                } else if ui.small_button("Rename").clicked() {
                    Click::Rename
                } else {
                    Click::None
                };
                ui.label(RichText::new(count_text(row.count)).color(tokens::TEXT_MUTED));
                (click, ui.available_rect_before_wrap())
            },
        )
        .inner;
    // The name takes the room left of the count and ends in an ellipsis.
    let name_rect = rest.with_max_x(rest.max.x - tokens::SPACING_S);
    let left_to_right = Layout::left_to_right(egui::Align::Center);
    ui.scope_builder(
        UiBuilder::new().max_rect(name_rect).layout(left_to_right),
        |ui| {
            ui.add(Label::new(RichText::new(&row.name)).truncate());
        },
    );
    click
}

/// Draw the field and the message of the row under edit. Enter commits,
/// Escape cancels.
fn edit_row(
    ui: &mut egui::Ui,
    rows: &[TagRow],
    row: &TagRow,
    edit: &mut Option<Edit>,
    events: &mut Vec<TagManagerEvent>,
) {
    // Consumed here, so the window does not see the same Escape and close.
    if ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
        *edit = None;
        return;
    }
    let Some(state) = edit.as_mut() else {
        return;
    };
    let id = ui.make_persistent_id(("tag-rename", row.id));
    if state.fresh {
        ui.data_mut(|d| d.remove::<TextEditState>(id));
        state.fresh = false;
    }
    let response = ui.add(
        TextEdit::singleline(&mut state.text)
            .id(id)
            .desired_width(f32::INFINITY),
    );
    let enter = response.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
    if enter {
        match judge(rows, row, &state.text) {
            Outcome::Same => {
                *edit = None;
                return;
            }
            Outcome::Rename(new_name) => {
                events.push(TagManagerEvent::Rename {
                    id: row.id,
                    new_name,
                });
                *edit = None;
                return;
            }
            Outcome::Refused(text) => state.error = Some(text),
        }
    }
    // Blur neither commits nor cancels; only Enter or Escape ends the edit.
    if !response.has_focus() {
        response.request_focus();
    }
    if let Some(error) = &state.error {
        ui.label(
            RichText::new(error)
                .size(tokens::SIZE_SMALL)
                .color(tokens::TAG_ERROR_TEXT),
        );
    }
}
