//! The Delete dialogs: a modal window that asks before files go to the Trash
//! or are deleted for good.
//!
//! There are three kinds. The Trash confirm defaults to "Move to Trash". The
//! permanent confirm and the "instead" prompt default to "Cancel". Enter never
//! confirms a permanent delete. Only the "Delete permanently" button does, by
//! click, or by Tab then Space.
//!
//! The dialog acts on the paths it was opened with. A change of the selection
//! while it is open does not change them.

use std::path::PathBuf;

use egui::{Align, Align2, Button, Context, Event, Key, Label, Layout, Modifiers, vec2};

use super::{Gallery, GridMenuEvent};
use crate::{backdrop, tokens};

/// Window width, in points.
const WIDTH: f32 = 360.0;

/// What the dialog asks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    /// Move the files to the Trash. The default button confirms.
    Trash,
    /// Delete the files for good. The default button cancels.
    Permanent,
    /// The Trash failed. Ask to delete for good. The default button cancels.
    Instead,
}

/// The state of the open dialog.
#[derive(Debug)]
pub(super) struct DeleteDialog {
    /// The files, as they were when the dialog opened.
    paths: Vec<PathBuf>,
    /// What the dialog asks.
    kind: Kind,
    /// Set until the default button has focus. Enter does nothing meanwhile.
    fresh: bool,
}

/// What one frame of the dialog decided.
enum Step {
    /// The dialog stays open.
    Stay,
    /// The dialog closes with no event.
    Close,
    /// The dialog closes and confirms the delete.
    Confirm,
}

/// The text for `count` files: "1 item" or "N items".
fn count_text(count: usize) -> String {
    if count == 1 {
        "1 item".to_owned()
    } else {
        format!("{count} items")
    }
}

impl DeleteDialog {
    /// Open the dialog of `kind` on `paths`.
    fn new(paths: Vec<PathBuf>, kind: Kind) -> Self {
        Self {
            paths,
            kind,
            fresh: true,
        }
    }

    /// The question. One file shows its name, more show their count.
    fn question(&self) -> String {
        let what = match self.paths.as_slice() {
            [one] => format!(
                "'{}'",
                one.file_name()
                    .map_or_else(|| one.to_string_lossy(), |name| name.to_string_lossy())
            ),
            many => count_text(many.len()),
        };
        match self.kind {
            Kind::Trash => format!("Move {what} to the Trash?"),
            Kind::Permanent => format!("Delete {what} permanently? This cannot be undone."),
            Kind::Instead => format!(
                "{} could not be moved to the Trash. Delete permanently instead? \
                 This cannot be undone.",
                count_text(self.paths.len())
            ),
        }
    }

    /// What Enter does, whatever widget has the focus: the Trash confirm
    /// confirms, and the permanent kinds close with no event.
    fn enter_step(&self) -> Step {
        match self.kind {
            Kind::Trash => Step::Confirm,
            Kind::Permanent | Kind::Instead => Step::Close,
        }
    }

    /// Draw the dialog and return what the user decided.
    fn show(&mut self, ctx: &Context) -> Step {
        backdrop::show(ctx, "tarragon-delete-backdrop");
        // Consumed here, so no other handler sees the same Escape.
        if ctx.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Escape)) {
            return Step::Close;
        }
        // Only a new press counts, never a repeat of a key that was already
        // down, and nothing counts before the default button has focus.
        if take_enter(ctx) && !self.fresh {
            return self.enter_step();
        }
        let mut step = Step::Stay;
        let title = match self.kind {
            Kind::Instead => "Delete permanently instead?",
            Kind::Trash | Kind::Permanent => "Delete",
        };
        egui::Window::new(title)
            .order(egui::Order::Foreground)
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.set_width(WIDTH);
                ui.add(Label::new(self.question()).wrap());
                ui.add_space(tokens::SPACING_S);
                let trash = self.kind == Kind::Trash;
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    // The first button added sits at the right edge. It is the
                    // default one and takes the focus. The Delete permanently
                    // button never has the focus when the dialog opens.
                    let (default, other) = if trash {
                        (ui.add(Button::new("Move to Trash")), ui.button("Cancel"))
                    } else {
                        (
                            ui.button("Cancel"),
                            ui.add(Button::new("Delete permanently")),
                        )
                    };
                    if self.fresh {
                        // A button keeps focus only from a frame that drew it,
                        // so the request repeats until the focus is there.
                        if ui.memory(|memory| memory.has_focus(default.id)) {
                            self.fresh = false;
                        } else {
                            default.request_focus();
                        }
                    }
                    let (confirm, cancel) = if trash {
                        (default.clicked(), other.clicked())
                    } else {
                        (other.clicked(), default.clicked())
                    };
                    if cancel {
                        step = Step::Close;
                    } else if confirm {
                        step = Step::Confirm;
                    }
                });
            });
        step
    }
}

/// Take every Enter key event out of the input, with any modifiers, so a
/// focused button never sees one. Returns whether one of them was a new press.
fn take_enter(ctx: &Context) -> bool {
    ctx.input_mut(|input| {
        let mut pressed = false;
        input.events.retain(|event| match event {
            Event::Key {
                key: Key::Enter,
                pressed: down,
                repeat,
                ..
            } => {
                pressed |= *down && !*repeat;
                false
            }
            _ => true,
        });
        pressed
    })
}

impl Gallery {
    /// Open the Delete confirm for `paths`: the permanent one when `permanent`
    /// is set, the Trash one otherwise. A tile menu that is open closes.
    ///
    /// Nothing opens for an empty list, or while a Delete or Rename dialog is
    /// open, so a second event cannot stack a dialog on the first.
    pub fn open_delete(&mut self, paths: Vec<PathBuf>, permanent: bool) {
        let kind = if permanent {
            Kind::Permanent
        } else {
            Kind::Trash
        };
        self.open_delete_kind(paths, kind);
    }

    /// Open the "Delete permanently instead?" prompt for the `paths` that the
    /// Trash could not take. It follows the rules of [`Gallery::open_delete`].
    pub fn open_delete_instead(&mut self, paths: Vec<PathBuf>) {
        self.open_delete_kind(paths, Kind::Instead);
    }

    /// Open the dialog of `kind` unless nothing may open.
    fn open_delete_kind(&mut self, paths: Vec<PathBuf>, kind: Kind) {
        if paths.is_empty() || self.delete.is_some() || self.rename.is_some() {
            return;
        }
        self.close_tile_menu();
        self.delete = Some(DeleteDialog::new(paths, kind));
    }

    /// Whether a Delete dialog is open.
    #[must_use]
    pub fn delete_open(&self) -> bool {
        self.delete.is_some()
    }

    /// Draw the Delete dialog when it is open. A confirm queues
    /// [`GridMenuEvent::ConfirmedDelete`].
    pub(super) fn show_delete(&mut self, ctx: &Context) {
        let Some(mut dialog) = self.delete.take() else {
            return;
        };
        match dialog.show(ctx) {
            Step::Stay => self.delete = Some(dialog),
            Step::Close => {}
            Step::Confirm => {
                self.tile_menu.push_event(GridMenuEvent::ConfirmedDelete {
                    permanent: dialog.kind != Kind::Trash,
                    paths: dialog.paths,
                });
            }
        }
    }
}
