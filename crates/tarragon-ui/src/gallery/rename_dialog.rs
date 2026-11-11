//! The Rename dialog: a modal window that asks for the new name of one file.
//!
//! The field holds the stem. The extension shows beside it as locked text.

use std::path::{Path, PathBuf};

use egui::text::{CCursor, CCursorRange};
use egui::text_edit::TextEditState;
use egui::{Align, Align2, Button, Context, Id, Key, Layout, Modifiers, RichText, TextEdit, vec2};
use tarragon_core::file_name::{
    FileNameError, MAX_FILE_NAME_BYTES, split_file_name, validate_file_stem,
};

use super::{Gallery, GridMenuEvent};
use crate::popup::text_width;
use crate::{backdrop, tokens};

/// Window width, in points.
const WIDTH: f32 = 360.0;

/// The id of the name field.
pub(super) const FIELD_ID: &str = "tarragon-rename-field";

/// The state of the open dialog.
#[derive(Debug)]
pub(super) struct RenameDialog {
    /// The file that gets the new name.
    path: PathBuf,
    /// The stem of the file when the dialog opened.
    stem: String,
    /// The extension of the file, without the dot.
    extension: Option<String>,
    /// The text of the field.
    text: String,
    /// Set until the field has focus and its stem is selected.
    fresh: bool,
}

/// What the typed stem amounts to.
enum Verdict {
    /// The stem did not change.
    Same,
    /// A valid new stem.
    Valid,
    /// The text to show under the field.
    Refused(String),
}

/// What one frame of the dialog decided.
enum Step {
    /// The dialog stays open.
    Stay,
    /// The dialog closes with no event.
    Close,
    /// The dialog closes and asks for this stem.
    Rename(String),
}

impl RenameDialog {
    /// Open the dialog on `path`, with its stem in the field.
    fn new(path: PathBuf) -> Self {
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let (stem, extension) = split_file_name(&name);
        let (stem, extension) = (stem.to_owned(), extension.map(str::to_owned));
        Self {
            path,
            text: stem.clone(),
            stem,
            extension,
            fresh: true,
        }
    }

    /// Judge the field. The same stem is checked first, because the current
    /// name of a file may fail the rules unchanged. `taken` says whether
    /// another file has a whole name.
    ///
    /// A name that ends in a dot has the extension `""`, and its whole name
    /// keeps the dot, so the length and `taken` checks add it back.
    fn verdict(&self, taken: impl Fn(&str) -> bool) -> Verdict {
        if self.text == self.stem {
            return Verdict::Same;
        }
        let extension = self.extension.as_deref().unwrap_or("");
        let dot = extension.is_empty() && self.extension.is_some();
        let whole = |name: &str| {
            if dot {
                format!("{name}.")
            } else {
                name.to_owned()
            }
        };
        let result = validate_file_stem(&self.text, extension, |_| false).and_then(|()| {
            if whole(&self.text).len() > MAX_FILE_NAME_BYTES {
                return Err(FileNameError::TooLong);
            }
            validate_file_stem(&self.text, extension, |name| taken(&whole(name)))
        });
        match result {
            Ok(()) => Verdict::Valid,
            Err(error) => Verdict::Refused(error.to_string()),
        }
    }

    /// Draw the dialog and return what the user decided.
    fn show(&mut self, ctx: &Context, taken: impl Fn(&str) -> bool) -> Step {
        backdrop::show(ctx, "tarragon-rename-backdrop");
        // Consumed here, so no other handler sees the same Escape.
        if ctx.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Escape)) {
            return Step::Close;
        }
        let verdict = self.verdict(taken);
        let mut step = Step::Stay;
        egui::Window::new("Rename")
            .order(egui::Order::Foreground)
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.set_width(WIDTH);
                let id = Id::new(FIELD_ID);
                if self.fresh {
                    // The field keeps a selection only while it has focus, so
                    // the selection waits for a frame that starts with focus.
                    if ui.memory(|memory| memory.has_focus(id)) {
                        select_all(ui.ctx(), id, &self.text);
                        self.fresh = false;
                    } else {
                        ui.memory_mut(|memory| memory.request_focus(id));
                    }
                }
                let suffix = self.extension.as_ref().map(|ext| format!(".{ext}"));
                let response = ui
                    .horizontal(|ui| {
                        let taken_width = suffix.as_deref().map_or(0.0, |text| {
                            text_width(ui, text) + ui.spacing().item_spacing.x
                        });
                        let room = (ui.available_width() - taken_width).max(0.0);
                        let response = ui.add(
                            TextEdit::singleline(&mut self.text)
                                .id(id)
                                .desired_width(room),
                        );
                        if let Some(text) = &suffix {
                            ui.label(RichText::new(text).color(tokens::TEXT_MUTED));
                        }
                        response
                    })
                    .inner;
                if let Verdict::Refused(text) = &verdict {
                    ui.label(
                        RichText::new(text)
                            .size(tokens::SIZE_SMALL)
                            .color(tokens::TAG_ERROR_TEXT),
                    );
                }
                let ok = !matches!(verdict, Verdict::Refused(_));
                let enter =
                    response.lost_focus() && ui.input(|input| input.key_pressed(Key::Enter));
                if enter && !ok {
                    response.request_focus();
                }
                let (mut clicked_ok, mut cancel) = (false, false);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    clicked_ok = ui.add_enabled(ok, Button::new("OK")).clicked();
                    cancel = ui.button("Cancel").clicked();
                });
                if cancel {
                    step = Step::Close;
                } else if ok && (clicked_ok || enter) {
                    step = match verdict {
                        Verdict::Valid => Step::Rename(self.text.clone()),
                        _ => Step::Close,
                    };
                }
            });
        step
    }
}

/// Select all of `text` in the field `id`, which drops the old cursor.
fn select_all(ctx: &Context, id: Id, text: &str) {
    let mut state = TextEditState::default();
    let end = CCursor::new(text.chars().count());
    state
        .cursor
        .set_char_range(Some(CCursorRange::two(CCursor::new(0), end)));
    state.store(ctx, id);
}

impl Gallery {
    /// Open the Rename dialog for the file at `path`. A tile menu that is open
    /// closes. Nothing opens while a Delete dialog or the tag picker is open.
    pub fn open_rename(&mut self, path: PathBuf) {
        if self.delete.is_some() || self.tag_picker.is_some() {
            return;
        }
        self.close_tile_menu();
        self.rename = Some(RenameDialog::new(path));
    }

    /// Whether the Rename dialog is open.
    #[must_use]
    pub fn rename_open(&self) -> bool {
        self.rename.is_some()
    }

    /// Draw the Rename dialog when it is open. A valid new stem queues
    /// [`GridMenuEvent::RenameTo`].
    pub(super) fn show_rename(&mut self, ctx: &Context) {
        let Some(mut dialog) = self.rename.take() else {
            return;
        };
        let folder = dialog.path.parent().map(Path::to_path_buf);
        let items = &self.items;
        let taken = |name: &str| {
            items.iter().any(|item| {
                item.path.parent() == folder.as_deref()
                    && item.path.file_name().is_some_and(|other| other == name)
            })
        };
        match dialog.show(ctx, taken) {
            Step::Stay => self.rename = Some(dialog),
            Step::Close => {}
            Step::Rename(new_stem) => {
                self.tile_menu.push_event(GridMenuEvent::RenameTo {
                    path: dialog.path,
                    new_stem,
                });
            }
        }
    }
}

#[cfg(test)]
impl RenameDialog {
    /// The text of the field.
    pub(super) fn text(&self) -> &str {
        &self.text
    }
}
