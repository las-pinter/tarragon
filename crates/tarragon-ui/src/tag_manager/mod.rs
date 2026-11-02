//! The "Manage tags" window: a list of the user tags with their image counts.
//!
//! The window floats inside the main window. It is not an OS window. The caller
//! owns the data and sets the rows.

use egui::{Align2, Key, Modifiers, vec2};

mod delete;
mod rename;

use rename::Edit;

/// Window width, in points.
const WIDTH: f32 = 360.0;

/// One user tag in the list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TagRow {
    /// The tag id.
    pub id: i64,
    /// The tag name.
    pub name: String,
    /// The number of images with the tag.
    pub count: u32,
}

/// A change the user asked for. The caller applies it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TagManagerEvent {
    /// Rename a tag to a name that passed the checks.
    Rename {
        /// The tag id.
        id: i64,
        /// The new, trimmed name.
        new_name: String,
    },
    /// Delete a tag.
    Delete {
        /// The tag id.
        id: i64,
    },
}

/// The state of the "Manage tags" window.
#[derive(Debug, Default)]
pub struct TagManager {
    open: bool,
    rows: Vec<TagRow>,
    edit: Option<Edit>,
    /// The id of the row that shows the delete question.
    confirm: Option<i64>,
    /// The row id and text of a failed delete.
    delete_error: Option<(i64, String)>,
    events: Vec<TagManagerEvent>,
    /// The last renamed tag id and the name sent for it.
    submitted: Option<(i64, String)>,
}

impl TagManager {
    /// Show the window from the next frame.
    pub fn open(&mut self) {
        self.open = true;
    }

    /// Hide the window.
    pub fn close(&mut self) {
        self.open = false;
        self.edit = None;
        self.confirm = None;
        self.delete_error = None;
    }

    /// Whether the window is shown.
    #[must_use]
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Replace the rows. The window stays as it is, open or closed. An edit or
    /// a delete question ends when its row is gone.
    pub fn set_rows(&mut self, rows: Vec<TagRow>) {
        self.rows = rows;
        if self
            .edit
            .as_ref()
            .is_some_and(|e| !self.rows.iter().any(|r| r.id == e.id()))
        {
            self.edit = None;
        }
        if self
            .confirm
            .is_some_and(|id| !self.rows.iter().any(|r| r.id == id))
        {
            self.confirm = None;
        }
        if self
            .delete_error
            .as_ref()
            .is_some_and(|(id, _)| !self.rows.iter().any(|r| r.id == *id))
        {
            self.delete_error = None;
        }
    }

    /// Take the changes the user asked for since the last call.
    pub fn take_events(&mut self) -> Vec<TagManagerEvent> {
        std::mem::take(&mut self.events)
    }

    /// Put the edit back on row `id` with `text` under the field. Call it when
    /// a rename was refused. It clears a delete message. A missing row is
    /// ignored.
    pub fn set_error(&mut self, id: i64, text: impl Into<String>) {
        self.delete_error = None;
        if let Some(row) = self.rows.iter().find(|r| r.id == id) {
            // The refused name goes back in the field, so the user can fix it.
            let name = match &self.submitted {
                Some((sent, name)) if *sent == id => name.clone(),
                _ => row.name.clone(),
            };
            self.edit = Some(Edit::with_error(row, name, text.into()));
            self.confirm = None;
        }
    }

    /// Show `text` on row `id` in place of the count. Call it when a delete
    /// was refused or failed. It ends any edit or delete question, on any row.
    /// The next Rename or Delete click, `set_error`, `set_rows` without the
    /// row, and `close` clear it. A message set while the window is closed
    /// shows when it opens; only `close` clears it. A missing row is ignored.
    pub fn set_delete_error(&mut self, id: i64, text: impl Into<String>) {
        if self.rows.iter().any(|r| r.id == id) {
            self.delete_error = Some((id, text.into()));
            self.edit = None;
            self.confirm = None;
        }
    }

    /// Draw the window when it is open.
    pub fn show(&mut self, ctx: &egui::Context) {
        if !self.open {
            return;
        }
        let mut open = true;
        egui::Window::new("Manage tags")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.set_width(WIDTH);
                let before = self.events.len();
                let (rows, edit, confirm) = (&self.rows, &mut self.edit, &mut self.confirm);
                let delete_error = &mut self.delete_error;
                rename::list(ui, rows, edit, confirm, delete_error, &mut self.events);
                if let Some(TagManagerEvent::Rename { id, new_name }) = self.events[before..].last()
                {
                    self.submitted = Some((*id, new_name.clone()));
                }
            });
        // An open rename field or delete question consumed its Escape, so only a
        // second one closes.
        let escape = ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape));
        self.open = open && !escape;
        if !self.open {
            self.edit = None;
            self.confirm = None;
            self.delete_error = None;
        }
    }
}

#[cfg(test)]
mod delete_error_tests;
#[cfg(test)]
mod delete_tests;
#[cfg(test)]
mod tests;
